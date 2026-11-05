//! Unified device model and the background monitor that keeps it current.
//! Every state shown in the UI is derived here from real adb, usbmuxd and USB responses.

use crate::android::adb::{Adb, AdbDevice, AdbState, DeviceProps};
use crate::android::session::MirrorManager;
use crate::ios::IosMonitor;
use crate::settings::Settings;
use crate::usb::{self, UsbKind, UsbPhone};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const POLL_INTERVAL: Duration = Duration::from_millis(1500);
/// How long a removed device stays listed as Disconnected.
const DISCONNECTED_RETENTION: Duration = Duration::from_secs(60);
/// scrcpy-server needs Android 5.0.
const MIN_MIRROR_SDK: u32 = 21;
/// Audio capture needs Android 11.
const MIN_AUDIO_SDK: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Platform {
    Android,
    Ios,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceState {
    Connected,
    Unauthorized,
    Offline,
    Disconnected,
    Unsupported,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub mirror: bool,
    pub control: bool,
    pub audio: bool,
    pub record: bool,
    pub screenshot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compatibility {
    pub level: &'static str,
    /// Stable identifier; the UI shows translated guidance when it knows the code.
    pub code: &'static str,
    pub message: &'static str,
}

impl Compatibility {
    pub fn info(code: &'static str, message: &'static str) -> Self {
        Self { level: "info", code, message }
    }
    pub fn warning(code: &'static str, message: &'static str) -> Self {
        Self { level: "warning", code, message }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub platform: Platform,
    pub name: String,
    pub model: Option<String>,
    pub manufacturer: Option<String>,
    pub os_version: Option<String>,
    pub state: DeviceState,
    pub capabilities: Capabilities,
    pub compatibility: Option<Compatibility>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSnapshot {
    pub devices: Vec<Device>,
    pub adb_error: Option<String>,
    /// Apple's usbmuxd service answered.
    pub apple_service: bool,
}

pub fn android_device(entry: &AdbDevice, props: Option<&DeviceProps>) -> Device {
    let sdk = props.and_then(|p| p.sdk);
    let (state, compatibility) = match entry.state {
        AdbState::Device => match sdk {
            Some(v) if v < MIN_MIRROR_SDK => (
                DeviceState::Unsupported,
                Some(Compatibility::warning(
                    "android_too_old",
                    "Screen mirroring requires Android 5.0 or later.",
                )),
            ),
            Some(v) if v < MIN_AUDIO_SDK => (
                DeviceState::Connected,
                Some(Compatibility::info(
                    "android_no_audio",
                    "Audio forwarding requires Android 11 or later. Video and control work normally.",
                )),
            ),
            _ => (DeviceState::Connected, None),
        },
        AdbState::Unauthorized => (
            DeviceState::Unauthorized,
            Some(Compatibility::warning(
                "android_unauthorized",
                "Unlock the phone and tap Allow on the \"Allow USB debugging?\" prompt. If it does \
                 not appear, unplug the cable and plug it back in.",
            )),
        ),
        AdbState::Offline => (
            DeviceState::Offline,
            Some(Compatibility::warning(
                "android_offline",
                "The device is not responding to adb. Unplug the cable and plug it back in, or \
                 restart the device.",
            )),
        ),
        AdbState::Other => (
            DeviceState::Offline,
            Some(Compatibility::warning(
                "android_not_booted",
                "The device is not fully booted. Finish starting it normally, then reconnect.",
            )),
        ),
    };

    let name = props
        .and_then(|p| p.market_name.clone().or_else(|| p.model.clone()))
        .or_else(|| entry.model.clone())
        .unwrap_or_else(|| "Android device".into());
    let ready = state == DeviceState::Connected;
    Device {
        id: entry.serial.clone(),
        platform: Platform::Android,
        name,
        model: props.and_then(|p| p.model.clone()).or_else(|| entry.model.clone()),
        manufacturer: props.and_then(|p| p.manufacturer.clone()),
        os_version: props.and_then(|p| p.android_version.as_ref().map(|v| format!("Android {v}"))),
        state,
        capabilities: Capabilities {
            mirror: ready,
            control: ready,
            audio: ready && sdk.is_some_and(|v| v >= MIN_AUDIO_SDK),
            record: ready,
            screenshot: ready,
        },
        compatibility,
    }
}

/// A phone visible on USB but not to adb. Usually USB debugging is off, but a stale adb connection
/// looks identical, so the message must not claim to know which.
fn debugging_off_device(phone: &UsbPhone) -> Device {
    let id = phone
        .serial
        .clone()
        .unwrap_or_else(|| format!("usb-{:04x}-{:04x}", phone.vendor_id, phone.product_id));
    Device {
        id,
        platform: Platform::Android,
        name: phone.product.clone().unwrap_or_else(|| "Android device".into()),
        model: None,
        manufacturer: phone.manufacturer.clone(),
        os_version: None,
        state: DeviceState::Offline,
        capabilities: Capabilities::default(),
        compatibility: Some(Compatibility::warning(
            "android_debugging_off",
            "This phone is connected but not responding to adb. Check that USB debugging is on in \
             Developer options, then unplug the cable and plug it back in.",
        )),
    }
}

/// Keep removed devices briefly as Disconnected so the user sees what happened to them.
pub fn merge_with_history(
    current: Vec<Device>,
    history: &mut HashMap<String, (Device, Instant)>,
    now: Instant,
) -> Vec<Device> {
    let mut devices = current;
    for device in &devices {
        history.insert(device.id.clone(), (device.clone(), now));
    }
    history.retain(|_, (_, seen)| now.duration_since(*seen) < DISCONNECTED_RETENTION);

    let mut gone: Vec<Device> = history
        .values()
        .filter(|(d, _)| !devices.iter().any(|c| c.id == d.id))
        .map(|(d, _)| Device {
            state: DeviceState::Disconnected,
            capabilities: Capabilities::default(),
            compatibility: None,
            ..d.clone()
        })
        .collect();
    gone.sort_by(|a, b| a.id.cmp(&b.id));
    devices.extend(gone);
    devices
}

pub struct DeviceMonitor {
    snapshot: Mutex<DeviceSnapshot>,
    rescan: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl DeviceMonitor {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { snapshot: Mutex::default(), rescan: Mutex::new(None) })
    }

    pub fn snapshot(&self) -> DeviceSnapshot {
        self.snapshot.lock().unwrap().clone()
    }

    pub fn request_scan(&self) {
        if let Some(tx) = self.rescan.lock().unwrap().as_ref() {
            let _ = tx.send(());
        }
    }

    pub fn spawn(
        self: &Arc<Self>,
        app: AppHandle,
        adb: Option<Arc<Adb>>,
        mirror: Option<Arc<MirrorManager>>,
        settings: Arc<Mutex<Settings>>,
        on_auto_connect: Arc<dyn Fn(&str) + Send + Sync>,
    ) {
        let (tx, rx) = std::sync::mpsc::channel();
        *self.rescan.lock().unwrap() = Some(tx);
        let monitor = self.clone();
        std::thread::spawn(move || {
            let ios = IosMonitor::default();
            let mut props: HashMap<String, DeviceProps> = HashMap::new();
            let mut history = HashMap::new();
            let mut started = adb.is_none();
            loop {
                let mut adb_error = None;
                if let Some(adb) = &adb {
                    if !started {
                        match adb.start_server() {
                            Ok(()) => started = true,
                            Err(e) => adb_error = Some(e.to_string()),
                        }
                    }
                } else {
                    adb_error = Some("adb was not found. Reinstall ScreenLink Studio.".to_string());
                }

                let usb_phones = usb::scan();
                let entries = match &adb {
                    Some(adb) if started => adb.devices().unwrap_or_else(|e| {
                        adb_error = Some(e.to_string());
                        started = false; // the daemon may have died; restart it next round
                        Vec::new()
                    }),
                    _ => Vec::new(),
                };

                let mut current = Vec::new();
                for entry in &entries {
                    if entry.state == AdbState::Device && !props.contains_key(&entry.serial) {
                        if let Some(p) = adb.as_ref().and_then(|a| a.props(&entry.serial).ok()) {
                            props.insert(entry.serial.clone(), p);
                        }
                    }
                    current.push(android_device(entry, props.get(&entry.serial)));
                }
                props.retain(|serial, _| entries.iter().any(|e| &e.serial == serial));

                if adb_error.is_none() {
                    for phone in usb_phones.iter().filter(|p| p.kind == UsbKind::Android) {
                        let known = phone
                            .serial
                            .as_ref()
                            .is_some_and(|s| entries.iter().any(|e| &e.serial == s));
                        if !known && entries.is_empty() {
                            current.push(debugging_off_device(phone));
                        }
                    }
                }

                let apple: Vec<&UsbPhone> =
                    usb_phones.iter().filter(|p| p.kind == UsbKind::Apple).collect();
                let (ios_devices, apple_service) = ios.scan(&apple);
                current.extend(ios_devices);

                let devices = merge_with_history(current, &mut history, Instant::now());
                let next = DeviceSnapshot { devices, adb_error, apple_service };

                let previous = {
                    let mut guard = monitor.snapshot.lock().unwrap();
                    std::mem::replace(&mut *guard, next.clone())
                };
                if previous != next {
                    let _ = app.emit("devices-changed", &next);
                    let auto = settings.lock().unwrap().auto_connect;
                    if auto && mirror.as_ref().is_some_and(|m| m.active_serial().is_none()) {
                        let newly_ready = next.devices.iter().find(|d| {
                            d.platform == Platform::Android
                                && d.capabilities.mirror
                                && !previous.devices.iter().any(|p| p.id == d.id && p.capabilities.mirror)
                        });
                        if let Some(device) = newly_ready {
                            on_auto_connect(&device.id);
                        }
                    }
                }

                // Wakes early when the UI requests a refresh.
                let _ = rx.recv_timeout(POLL_INTERVAL);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(state: AdbState) -> AdbDevice {
        AdbDevice { serial: "ABC".into(), state, model: Some("Pixel 8".into()) }
    }

    fn props(sdk: u32) -> DeviceProps {
        DeviceProps {
            manufacturer: Some("Google".into()),
            model: Some("Pixel 8".into()),
            market_name: None,
            android_version: Some("14".into()),
            sdk: Some(sdk),
        }
    }

    #[test]
    fn connected_device_has_full_capabilities() {
        let d = android_device(&entry(AdbState::Device), Some(&props(34)));
        assert_eq!(d.state, DeviceState::Connected);
        assert!(d.capabilities.mirror && d.capabilities.audio && d.capabilities.control);
        assert_eq!(d.os_version.as_deref(), Some("Android 14"));
        assert!(d.compatibility.is_none());
    }

    #[test]
    fn unauthorized_is_not_mirrorable_and_explains_itself() {
        let d = android_device(&entry(AdbState::Unauthorized), None);
        assert_eq!(d.state, DeviceState::Unauthorized);
        assert!(!d.capabilities.mirror);
        assert_eq!(d.compatibility.unwrap().code, "android_unauthorized");
    }

    #[test]
    fn offline_and_transitional_states() {
        assert_eq!(android_device(&entry(AdbState::Offline), None).state, DeviceState::Offline);
        let d = android_device(&entry(AdbState::Other), None);
        assert_eq!(d.state, DeviceState::Offline);
        assert_eq!(d.compatibility.unwrap().code, "android_not_booted");
    }

    #[test]
    fn old_android_is_unsupported_and_mid_android_loses_audio() {
        assert_eq!(
            android_device(&entry(AdbState::Device), Some(&props(19))).state,
            DeviceState::Unsupported
        );
        let d = android_device(&entry(AdbState::Device), Some(&props(29)));
        assert!(d.capabilities.mirror && !d.capabilities.audio);
        assert_eq!(d.compatibility.unwrap().code, "android_no_audio");
    }

    #[test]
    fn removed_device_becomes_disconnected_then_expires() {
        let d = android_device(&entry(AdbState::Device), Some(&props(34)));
        let mut history = HashMap::new();
        let t0 = Instant::now();
        assert_eq!(merge_with_history(vec![d], &mut history, t0).len(), 1);

        let gone = merge_with_history(vec![], &mut history, t0 + Duration::from_secs(5));
        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].state, DeviceState::Disconnected);
        assert!(!gone[0].capabilities.mirror);

        let later = merge_with_history(vec![], &mut history, t0 + DISCONNECTED_RETENTION * 2);
        assert!(later.is_empty());
    }
}
