//! Loopback WebSocket bridge between Rust and the webview.
//!
//! Compressed media goes straight to WebCodecs in the page. A socket is used rather than Tauri IPC
//! because IPC adds per-message overhead that shows up as latency at 60 fps. It binds to 127.0.0.1
//! only and every connection must present a per-session 256-bit token and an app origin.
//!
//! Two sockets, each one-directional, keep the blocking I/O simple:
//!   /media  server -> page  framed binary packets
//!   /input  page -> server  JSON input events

use rand::RngCore;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::StatusCode;
use tungstenite::{accept_hdr, Message};

pub const KIND_VIDEO: u8 = 0;
pub const KIND_AUDIO: u8 = 1;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub const KIND_JPEG: u8 = 2;
pub const FLAG_CONFIG: u8 = 1;
pub const FLAG_KEY: u8 = 2;

/// About two seconds of 60 fps video.
const QUEUE_CAPACITY: usize = 128;

const ALLOWED_ORIGINS: [&str; 3] =
    ["tauri://localhost", "http://tauri.localhost", "https://tauri.localhost"];

/// Frame layout: kind u8, flags u8, pts u64 (big endian), payload.
pub fn frame(kind: u8, flags: u8, pts_us: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(10 + payload.len());
    out.push(kind);
    out.push(flags);
    out.extend(pts_us.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

pub trait BridgeHandler: Send + Sync + 'static {
    /// A page connected to /media. `reconnect` means earlier packets were not seen by this page,
    /// so it needs a fresh key frame.
    fn media_connected(&self, reconnect: bool);
    fn input(&self, json: &str);
}

pub struct Bridge {
    pub port: u16,
    pub token: String,
    sender: SyncSender<Vec<u8>>,
    shutdown: Arc<AtomicBool>,
}

impl Bridge {
    pub fn start(handler: Arc<dyn BridgeHandler>) -> std::io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();

        let mut raw = [0u8; 32];
        rand::rng().fill_bytes(&mut raw);
        let token: String = raw.iter().map(|b| format!("{b:02x}")).collect();

        let (sender, receiver) = sync_channel(QUEUE_CAPACITY);
        let shutdown = Arc::new(AtomicBool::new(false));
        let ctx = Arc::new(Context {
            token: token.clone(),
            receiver: Mutex::new(receiver),
            handler,
            shutdown: shutdown.clone(),
            media_generation: AtomicU64::new(0),
        });
        std::thread::spawn(move || accept_loop(listener, ctx));
        Ok(Self { port, token, sender, shutdown })
    }

    /// Non-blocking so a stalled page can never back-pressure the device sockets. Returns false
    /// when the packet was dropped.
    pub fn send(&self, data: Vec<u8>) -> bool {
        match self.sender.try_send(data) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
        }
    }

    pub fn close(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.close();
    }
}

struct Context {
    token: String,
    receiver: Mutex<Receiver<Vec<u8>>>,
    handler: Arc<dyn BridgeHandler>,
    shutdown: Arc<AtomicBool>,
    media_generation: AtomicU64,
}

fn accept_loop(listener: TcpListener, ctx: Arc<Context>) {
    while !ctx.shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let ctx = ctx.clone();
                std::thread::spawn(move || handle_connection(stream, ctx));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(e) => {
                log::warn!("bridge accept failed: {e}");
                return;
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Route {
    Media,
    Input,
}

fn authorize(path_and_query: &str, origin: Option<&str>, token: &str) -> Option<Route> {
    let dev_server = cfg!(debug_assertions) && origin == Some("http://localhost:1420");
    if !dev_server && !origin.is_some_and(|o| ALLOWED_ORIGINS.contains(&o)) {
        return None;
    }
    let (path, query) = path_and_query.split_once('?')?;
    let supplied = query.split('&').find_map(|kv| kv.strip_prefix("token="))?;
    if !constant_time_eq(supplied.as_bytes(), token.as_bytes()) {
        return None;
    }
    match path {
        "/media" => Some(Route::Media),
        "/input" => Some(Route::Input),
        _ => None,
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn handle_connection(stream: TcpStream, ctx: Arc<Context>) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));

    let mut route = None;
    let token = ctx.token.clone();
    let callback = |req: &Request, res: Response| -> Result<Response, ErrorResponse> {
        let origin = req.headers().get("origin").and_then(|v| v.to_str().ok());
        let path = req.uri().path_and_query().map_or("", |p| p.as_str());
        route = authorize(path, origin, &token);
        if route.is_some() {
            Ok(res)
        } else {
            let mut denied = ErrorResponse::new(None);
            *denied.status_mut() = StatusCode::FORBIDDEN;
            Err(denied)
        }
    };
    let Ok(socket) = accept_hdr(stream, callback) else { return };

    match route {
        Some(Route::Media) => media_loop(socket, ctx),
        Some(Route::Input) => input_loop(socket, ctx),
        None => {}
    }
}

fn media_loop(mut socket: tungstenite::WebSocket<TcpStream>, ctx: Arc<Context>) {
    let generation = ctx.media_generation.fetch_add(1, Ordering::SeqCst) + 1;
    ctx.handler.media_connected(generation > 1);

    loop {
        if ctx.shutdown.load(Ordering::SeqCst)
            || ctx.media_generation.load(Ordering::SeqCst) != generation
        {
            break;
        }
        // The lock is only contended during the moment a reconnect replaces an old socket.
        let next = ctx
            .receiver
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_millis(100));
        match next {
            Ok(data) => {
                if socket.send(Message::Binary(data.into())).is_err() {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = socket.close(None);
}

fn input_loop(mut socket: tungstenite::WebSocket<TcpStream>, ctx: Arc<Context>) {
    // Short timeout so the shutdown flag is noticed; timeouts surface as WouldBlock/TimedOut.
    let _ = socket.get_ref().set_read_timeout(Some(Duration::from_millis(250)));
    loop {
        if ctx.shutdown.load(Ordering::SeqCst) {
            break;
        }
        match socket.read() {
            Ok(Message::Text(text)) => ctx.handler.input(text.as_str()),
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    let _ = socket.close(None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout() {
        assert_eq!(
            frame(KIND_VIDEO, FLAG_KEY, 0x0102, &[9, 8]),
            [0, 2, 0, 0, 0, 0, 0, 0, 1, 2, 9, 8]
        );
    }

    #[test]
    fn authorize_requires_token_and_origin() {
        let t = "abc123";
        let o = Some("tauri://localhost");
        assert!(authorize("/media?token=abc123", o, t) == Some(Route::Media));
        assert!(authorize("/input?token=abc123", Some("http://tauri.localhost"), t) == Some(Route::Input));
        assert!(authorize("/media?token=nope", o, t).is_none());
        assert!(authorize("/media?token=", o, t).is_none());
        assert!(authorize("/media", o, t).is_none());
        assert!(authorize("/other?token=abc123", o, t).is_none());
        assert!(authorize("/media?token=abc123", None, t).is_none());
        assert!(authorize("/media?token=abc123", Some("https://evil.example"), t).is_none());
    }

    struct Probe;
    impl BridgeHandler for Probe {
        fn media_connected(&self, _: bool) {}
        fn input(&self, _: &str) {}
    }

    #[test]
    fn rejects_unauthenticated_websocket_and_binds_loopback() {
        let bridge = Bridge::start(Arc::new(Probe)).unwrap();
        let url = format!("ws://127.0.0.1:{}/media?token=wrong", bridge.port);
        let mut req = tungstenite::client::IntoClientRequest::into_client_request(url).unwrap();
        req.headers_mut().insert("origin", "tauri://localhost".parse().unwrap());
        let stream = TcpStream::connect(("127.0.0.1", bridge.port)).unwrap();
        assert!(tungstenite::client(req, stream).is_err());
    }

    #[test]
    fn delivers_media_to_authenticated_client() {
        let bridge = Bridge::start(Arc::new(Probe)).unwrap();
        assert!(bridge.send(frame(KIND_VIDEO, 0, 7, &[1, 2, 3])));
        let url = format!("ws://127.0.0.1:{}/media?token={}", bridge.port, bridge.token);
        let mut req = tungstenite::client::IntoClientRequest::into_client_request(url).unwrap();
        req.headers_mut().insert("origin", "tauri://localhost".parse().unwrap());
        let stream = TcpStream::connect(("127.0.0.1", bridge.port)).unwrap();
        let (mut socket, _) = tungstenite::client(req, stream).unwrap();
        let msg = socket.read().unwrap();
        assert_eq!(msg.into_data().as_ref(), frame(KIND_VIDEO, 0, 7, &[1, 2, 3]).as_slice());
    }
}
