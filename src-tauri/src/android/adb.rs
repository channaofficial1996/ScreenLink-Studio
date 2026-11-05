//! Thin wrapper around the adb executable. Arguments are always passed as a discrete argv, never
//! through a shell, and every call has a timeout so a wedged USB link cannot hang the app.

use crate::error::{AppError, AppResult};
use crate::process::{hide_window, run_with_timeout};
use crate::validate::validate_serial;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const SHORT_TIMEOUT: Duration = Duration::from_secs(6);
const PUSH_TIMEOUT: Duration = Duration::from_secs(30);
const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(15);
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

#[cfg(windows)]
const ADB_FILE: &str = "adb.exe";
#[cfg(not(windows))]
const ADB_FILE: &str = "adb";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdbState {
    Device,
    Unauthorized,
    Offline,
    /// Fastboot-adjacent or transitional modes (recovery, sideload, connecting, …).
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbDevice {
    pub serial: String,
    pub state: AdbState,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceProps {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub market_name: Option<String>,
    pub android_version: Option<String>,
    pub sdk: Option<u32>,
}

pub struct Adb {
    path: PathBuf,
    started_daemon: AtomicBool,
}

impl Adb {
    /// Prefer the bundled adb so the version is known-good; fall back to a system install.
    pub fn locate(resource_dir: Option<&Path>) -> AppResult<Self> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(dir) = resource_dir {
            candidates.push(dir.join("resources").join("platform-tools").join(ADB_FILE));
        }
        candidates.push(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join("platform-tools")
                .join(ADB_FILE),
        );
        for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
            if let Some(sdk) = std::env::var_os(var) {
                candidates.push(PathBuf::from(sdk).join("platform-tools").join(ADB_FILE));
            }
        }
        if let Some(paths) = std::env::var_os("PATH") {
            candidates.extend(std::env::split_paths(&paths).map(|p| p.join(ADB_FILE)));
        }
        candidates
            .into_iter()
            .find(|p| p.is_absolute() && p.is_file())
            .map(|path| Self { path, started_daemon: AtomicBool::new(false) })
            .ok_or_else(|| AppError::AdbUnavailable("adb was not found.".into()))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn raw(&self, args: &[&str], timeout: Duration) -> AppResult<Output> {
        let mut cmd = Command::new(&self.path);
        cmd.args(args);
        run_with_timeout(cmd, timeout)
    }

    fn for_device(&self, serial: &str, args: &[&str], timeout: Duration) -> AppResult<Output> {
        validate_serial(serial)?;
        let mut full = vec!["-s", serial];
        full.extend_from_slice(args);
        self.raw(&full, timeout)
    }

    pub fn start_server(&self) -> AppResult<()> {
        let out = self.raw(&["start-server"], Duration::from_secs(20))?;
        if !out.status.success() {
            return Err(AppError::AdbUnavailable(stderr(&out)));
        }
        if String::from_utf8_lossy(&out.stderr).contains("daemon started successfully") {
            self.started_daemon.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Stop the daemon only if this app started it, so the install directory is not left locked
    /// and other Android tooling the user was already running is left alone.
    pub fn stop_server_if_owned(&self) {
        if self.started_daemon.load(Ordering::SeqCst) {
            let _ = self.raw(&["kill-server"], Duration::from_secs(5));
        }
    }

    pub fn stop_server(&self) {
        let _ = self.raw(&["kill-server"], Duration::from_secs(5));
    }

    pub fn version(&self) -> Option<String> {
        let out = self.raw(&["version"], SHORT_TIMEOUT).ok()?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|l| l.strip_prefix("Version ").map(|v| v.trim().to_string()))
    }

    pub fn devices(&self) -> AppResult<Vec<AdbDevice>> {
        let out = self.raw(&["devices", "-l"], SHORT_TIMEOUT)?;
        if !out.status.success() {
            return Err(AppError::Adb(stderr(&out)));
        }
        Ok(parse_devices(&String::from_utf8_lossy(&out.stdout)))
    }

    pub fn props(&self, serial: &str) -> AppResult<DeviceProps> {
        let out = self.for_device(serial, &["shell", "getprop"], SHORT_TIMEOUT)?;
        if !out.status.success() {
            return Err(AppError::Adb(stderr(&out)));
        }
        Ok(parse_props(&String::from_utf8_lossy(&out.stdout)))
    }

    pub fn push(&self, serial: &str, local: &Path, remote: &str) -> AppResult<()> {
        let local = local
            .to_str()
            .ok_or_else(|| AppError::Mirror("Invalid scrcpy-server path.".into()))?;
        let out = self.for_device(serial, &["push", local, remote], PUSH_TIMEOUT)?;
        check(&out)
    }

    /// Forward to an abstract socket on an OS-assigned local port, returning that port.
    pub fn forward(&self, serial: &str, socket: &str) -> AppResult<u16> {
        let remote = format!("localabstract:{socket}");
        let out = self.for_device(serial, &["forward", "tcp:0", &remote], SHORT_TIMEOUT)?;
        check(&out)?;
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .map_err(|_| AppError::Adb("adb did not report a forwarded port.".into()))
    }

    pub fn remove_forward(&self, serial: &str, port: u16) {
        let local = format!("tcp:{port}");
        let _ = self.for_device(serial, &["forward", "--remove", &local], SHORT_TIMEOUT);
    }

    /// Start a long-running `adb -s <serial> shell …` with stdout captured.
    pub fn spawn_shell(&self, serial: &str, shell_args: &[String]) -> AppResult<std::process::Child> {
        validate_serial(serial)?;
        let mut cmd = Command::new(&self.path);
        cmd.args(["-s", serial, "shell"]).args(shell_args);
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        hide_window(&mut cmd);
        cmd.spawn().map_err(|e| AppError::AdbUnavailable(e.to_string()))
    }

    /// Full-resolution screenshot from the device framebuffer, independent of any mirror session.
    pub fn screenshot(&self, serial: &str) -> AppResult<Vec<u8>> {
        let out = self.for_device(serial, &["exec-out", "screencap", "-p"], SCREENSHOT_TIMEOUT)?;
        check(&out)?;
        if !out.stdout.starts_with(PNG_SIGNATURE) {
            return Err(AppError::Adb("The device did not return a screenshot.".into()));
        }
        Ok(out.stdout)
    }
}

fn check(out: &Output) -> AppResult<()> {
    if out.status.success() {
        Ok(())
    } else {
        Err(AppError::Adb(stderr(out)))
    }
}

fn stderr(out: &Output) -> String {
    let text = String::from_utf8_lossy(&out.stderr);
    let text = text.trim();
    if text.is_empty() {
        "adb reported an error.".to_string()
    } else {
        text.to_string()
    }
}

pub fn parse_devices(output: &str) -> Vec<AdbDevice> {
    output
        .lines()
        .skip_while(|l| !l.starts_with("List of devices"))
        .skip(1)
        .filter_map(parse_device_line)
        .collect()
}

fn parse_device_line(line: &str) -> Option<AdbDevice> {
    let mut parts = line.split_whitespace();
    let serial = parts.next()?.to_string();
    if !is_usb_serial(&serial) {
        return None;
    }
    let state = match parts.next()? {
        "device" => AdbState::Device,
        "unauthorized" | "authorizing" => AdbState::Unauthorized,
        "offline" => AdbState::Offline,
        _ => AdbState::Other,
    };
    let model = parts
        .find_map(|p| p.strip_prefix("model:"))
        .map(|m| m.replace('_', " "));
    Some(AdbDevice { serial, state, model })
}

/// USB-only release: ignore wireless debugging transports (`ip:port`, `adb-…._adb-tls-connect`).
fn is_usb_serial(serial: &str) -> bool {
    !serial.contains(':') && !serial.contains("._adb")
}

pub fn parse_props(output: &str) -> DeviceProps {
    let map: HashMap<&str, &str> = output
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once("]: [")?;
            Some((k.strip_prefix('[')?, v.trim_end().strip_suffix(']')?))
        })
        .collect();
    let get = |k: &str| map.get(k).filter(|v| !v.is_empty()).map(|v| v.to_string());
    DeviceProps {
        manufacturer: get("ro.product.manufacturer"),
        model: get("ro.product.model"),
        market_name: get("ro.product.marketname").or_else(|| get("ro.vendor.product.display")),
        android_version: get("ro.build.version.release"),
        sdk: get("ro.build.version.sdk").and_then(|v| v.parse().ok()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unauthorized_line_from_real_device() {
        // Captured verbatim from a moto g54 before approving the USB debugging prompt.
        let out = "List of devices attached\nNKQL4D0162             unauthorized transport_id:1\n\n";
        assert_eq!(
            parse_devices(out),
            vec![AdbDevice {
                serial: "NKQL4D0162".into(),
                state: AdbState::Unauthorized,
                model: None
            }]
        );
    }

    #[test]
    fn parses_multiple_devices_and_states() {
        let out = "* daemon not running; starting now at tcp:5037\n\
                   * daemon started successfully\n\
                   List of devices attached\n\
                   AAA111 device product:cancunf_g model:moto_g54_5G device:cancunf transport_id:1\n\
                   BBB222 offline transport_id:2\n\
                   CCC333 recovery transport_id:3\n";
        let d = parse_devices(out);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].state, AdbState::Device);
        assert_eq!(d[0].model.as_deref(), Some("moto g54 5G"));
        assert_eq!(d[1].state, AdbState::Offline);
        assert_eq!(d[2].state, AdbState::Other);
    }

    #[test]
    fn ignores_wireless_transports_and_noise() {
        let out = "List of devices attached\n\
                   192.168.1.20:5555 device product:x model:y device:z transport_id:4\n\
                   adb-R5CT._adb-tls-connect._tcp device transport_id:5\n\
                   \n";
        assert!(parse_devices(out).is_empty());
        assert!(parse_devices("").is_empty());
    }

    #[test]
    fn parses_getprop_output() {
        let out = "[ro.product.manufacturer]: [motorola]\r\n\
                   [ro.product.model]: [moto g54 5G]\n\
                   [ro.build.version.release]: [14]\n\
                   [ro.build.version.sdk]: [34]\n\
                   [ro.product.marketname]: []\n\
                   [garbage line]\n";
        let p = parse_props(out);
        assert_eq!(p.manufacturer.as_deref(), Some("motorola"));
        assert_eq!(p.model.as_deref(), Some("moto g54 5G"));
        assert_eq!(p.android_version.as_deref(), Some("14"));
        assert_eq!(p.sdk, Some(34));
        assert_eq!(p.market_name, None);
    }
}
