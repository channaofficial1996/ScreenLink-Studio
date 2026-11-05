//! Deploys and connects to scrcpy-server on the device. We run scrcpy's official server unchanged
//! and act as the client ourselves, which is what lets the video render inside the app.
//!
//! The wire protocol is internal to scrcpy and changes between releases, so the version here is
//! pinned and must match the jar fetched by scripts/fetch-sidecars.mjs.

use super::adb::Adb;
use crate::error::{AppError, AppResult};
use crate::process;
use crate::validate::redact;
use std::io::{BufRead, BufReader, Read};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const SCRCPY_VERSION: &str = "5.0.1";
const DEVICE_SERVER_PATH: &str = "/data/local/tmp/screenlink-server.jar";
const DEVICE_NAME_LENGTH: usize = 64;
const CONNECT_ATTEMPTS: u32 = 100;
const CONNECT_DELAY: Duration = Duration::from_millis(100);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOptions {
    /// Longest edge in pixels; 0 keeps the device resolution.
    pub max_size: u32,
    pub max_fps: u32,
    pub video_bit_rate: u32,
    pub audio: bool,
    pub stay_awake: bool,
    pub show_touches: bool,
}

/// The `key=value` arguments for this session. Every value is numeric or a fixed literal, so
/// nothing user-controlled ever reaches the device shell.
pub fn server_args(scid: u32, o: &ServerOptions) -> Vec<String> {
    let mut args: Vec<String> = [
        format!("CLASSPATH={DEVICE_SERVER_PATH}"),
        "app_process".into(),
        "/".into(),
        "com.genymobile.scrcpy.Server".into(),
        SCRCPY_VERSION.into(),
        format!("scid={scid:08x}"),
        "log_level=info".into(),
        "tunnel_forward=true".into(),
        "video_codec=h264".into(),
        "clipboard_autosync=false".into(),
        format!("video_bit_rate={}", o.video_bit_rate),
        format!("audio={}", o.audio),
    ]
    .into();
    if o.audio {
        args.push("audio_codec=opus".into());
    }
    if o.max_size > 0 {
        args.push(format!("max_size={}", o.max_size));
    }
    if o.max_fps > 0 {
        args.push(format!("max_fps={}", o.max_fps));
    }
    if o.stay_awake {
        args.push("stay_awake=true".into());
    }
    if o.show_touches {
        args.push("show_touches=true".into());
    }
    args
}

pub struct ServerConnection {
    pub child: Child,
    pub port: u16,
    pub video: TcpStream,
    pub audio: Option<TcpStream>,
    pub control: TcpStream,
    pub device_name: String,
}

pub fn launch(
    adb: &Adb,
    serial: &str,
    jar: &Path,
    options: &ServerOptions,
    mut progress: impl FnMut(&'static str),
) -> AppResult<ServerConnection> {
    progress("pushing");
    adb.push(serial, jar, DEVICE_SERVER_PATH)?;

    let scid: u32 = rand::random::<u32>() & 0x7fff_ffff;
    let port = adb.forward(serial, &format!("scrcpy_{scid:08x}"))?;

    progress("launching");
    log::info!("starting scrcpy-server on {}", redact(serial));
    let mut child = match adb.spawn_shell(serial, &server_args(scid, options)) {
        Ok(c) => c,
        Err(e) => {
            adb.remove_forward(serial, port);
            return Err(e);
        }
    };
    process::adopt(&child);
    let logs = capture_logs(&mut child);

    progress("connecting");
    match connect_all(&mut child, port, options.audio, &logs) {
        Ok((video, audio, control, device_name)) => Ok(ServerConnection {
            child,
            port,
            video,
            audio,
            control,
            device_name,
        }),
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            adb.remove_forward(serial, port);
            Err(e)
        }
    }
}

/// The server's stdout carries its log; keep the tail so failures can be explained to the user.
fn capture_logs(child: &mut Child) -> Arc<Mutex<Vec<String>>> {
    let logs: Arc<Mutex<Vec<String>>> = Arc::default();
    let streams: [Option<Box<dyn Read + Send>>; 2] = [
        child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
        child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
    ];
    for stream in streams.into_iter().flatten() {
        let sink = logs.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                log::debug!("[scrcpy-server] {line}");
                let mut sink = sink.lock().unwrap();
                if sink.len() >= 40 {
                    sink.remove(0);
                }
                sink.push(line);
            }
        });
    }
    logs
}

type Sockets = (TcpStream, Option<TcpStream>, TcpStream, String);

fn connect_all(
    child: &mut Child,
    port: u16,
    audio: bool,
    logs: &Arc<Mutex<Vec<String>>>,
) -> AppResult<Sockets> {
    let mut video = connect_first(child, port, logs)?;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));

    let audio = if audio {
        let s = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
        s.set_nodelay(true)?;
        Some(s)
    } else {
        None
    };
    let control = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
    control.set_nodelay(true)?;

    // Metadata arrives on the first socket once all of them are connected.
    video.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut name = [0u8; DEVICE_NAME_LENGTH];
    video.read_exact(&mut name).map_err(|_| server_failure(logs))?;
    video.set_read_timeout(None)?;
    let end = name.iter().position(|b| *b == 0).unwrap_or(DEVICE_NAME_LENGTH);
    Ok((video, audio, control, String::from_utf8_lossy(&name[..end]).into_owned()))
}

/// adb accepts the TCP connection even when nothing is listening on the device, so the server is
/// only ready once its dummy byte arrives.
fn connect_first(
    child: &mut Child,
    port: u16,
    logs: &Arc<Mutex<Vec<String>>>,
) -> AppResult<TcpStream> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    for _ in 0..CONNECT_ATTEMPTS {
        if let Ok(Some(_)) = child.try_wait() {
            return Err(server_failure(logs));
        }
        if let Ok(mut stream) = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            let _ = stream.set_nodelay(true);
            let _ = stream.set_read_timeout(Some(CONNECT_TIMEOUT));
            let mut byte = [0u8; 1];
            if matches!(stream.read(&mut byte), Ok(1)) {
                return Ok(stream);
            }
        }
        std::thread::sleep(CONNECT_DELAY);
    }
    Err(server_failure(logs))
}

fn server_failure(logs: &Arc<Mutex<Vec<String>>>) -> AppError {
    // Give the log reader a moment: the exit is usually observed before its last lines arrive.
    std::thread::sleep(Duration::from_millis(150));
    let logs = logs.lock().unwrap();
    let reason = logs.iter().rev().find(|l| l.contains("ERROR")).cloned();
    AppError::Mirror(match reason {
        Some(line) => format!("The device could not start mirroring: {}", line.trim()),
        None => "The device could not start mirroring. Reconnect the cable and try again.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> ServerOptions {
        ServerOptions {
            max_size: 1920,
            max_fps: 60,
            video_bit_rate: 8_000_000,
            audio: true,
            stay_awake: false,
            show_touches: false,
        }
    }

    #[test]
    fn builds_expected_arguments() {
        let a = server_args(0x1234abcd, &options());
        assert_eq!(a[0], "CLASSPATH=/data/local/tmp/screenlink-server.jar");
        assert_eq!(a[3], "com.genymobile.scrcpy.Server");
        assert_eq!(a[4], "5.0.1");
        for want in [
            "scid=1234abcd",
            "tunnel_forward=true",
            "video_codec=h264",
            "audio=true",
            "audio_codec=opus",
            "video_bit_rate=8000000",
            "max_size=1920",
            "max_fps=60",
            "clipboard_autosync=false",
        ] {
            assert!(a.iter().any(|x| x == want), "missing {want}");
        }
    }

    #[test]
    fn scid_is_zero_padded() {
        assert!(server_args(0xab, &options()).iter().any(|x| x == "scid=000000ab"));
    }

    #[test]
    fn omits_disabled_options() {
        let mut o = options();
        o.audio = false;
        o.max_size = 0;
        o.max_fps = 0;
        o.stay_awake = true;
        let a = server_args(1, &o);
        assert!(a.iter().any(|x| x == "audio=false"));
        assert!(a.iter().any(|x| x == "stay_awake=true"));
        assert!(!a.iter().any(|x| x.starts_with("audio_codec")));
        assert!(!a.iter().any(|x| x.starts_with("max_size")));
        assert!(!a.iter().any(|x| x.starts_with("max_fps")));
    }

    #[test]
    fn version_matches_fetch_script() {
        let script = include_str!("../../../scripts/fetch-sidecars.mjs");
        assert!(script.contains(&format!("SCRCPY_VERSION = \"{SCRCPY_VERSION}\"")));
    }
}
