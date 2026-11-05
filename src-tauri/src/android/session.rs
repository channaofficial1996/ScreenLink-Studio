//! Owns the single active mirroring session: its server process, sockets, threads and recorder.
//!
//! At most one session exists at a time, so duplicate server processes are impossible by
//! construction. Every exit path (user stop, USB disconnect, app exit) runs `stop_session`, which
//! finalizes any recording and removes the adb forward.

use super::adb::Adb;
use super::control::InputEvent;
use super::server::{self, ServerOptions};
use super::stream::{self, Packet, StreamEvent};
use crate::bridge::{frame, Bridge, BridgeHandler, FLAG_CONFIG, FLAG_KEY, KIND_AUDIO, KIND_VIDEO};
use crate::error::{AppError, AppResult};
use crate::recording::Recorder;
use crate::validate::{redact, validate_serial};
use serde::Serialize;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MirrorPhase {
    Idle,
    Starting,
    Running,
    Stopping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioState {
    Off,
    Pending,
    Active,
    /// The device refused audio capture (Android below 11, or a secured audio policy).
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorStatus {
    pub phase: MirrorPhase,
    pub step: Option<&'static str>,
    pub serial: Option<String>,
    pub device_name: Option<String>,
    pub width: u32,
    pub height: u32,
    pub audio: AudioState,
    pub port: u16,
    pub token: String,
    /// Why the last session ended, when it was not the user's choice.
    pub ended_reason: Option<String>,
}

impl MirrorStatus {
    pub fn idle() -> Self {
        Self {
            phase: MirrorPhase::Idle,
            step: None,
            serial: None,
            device_name: None,
            width: 0,
            height: 0,
            audio: AudioState::Off,
            port: 0,
            token: String::new(),
            ended_reason: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub active: bool,
    pub started_at_ms: Option<i64>,
    pub files: Vec<String>,
    pub error: Option<String>,
}

struct InputSink {
    control: Sender<Vec<u8>>,
}

impl InputSink {
    fn request_key_frame(&self) {
        if let Some(bytes) = InputEvent::ResetVideo.encode() {
            let _ = self.control.send(bytes);
        }
    }
}

impl BridgeHandler for InputSink {
    fn media_connected(&self, reconnect: bool) {
        if reconnect {
            self.request_key_frame();
        }
    }

    fn input(&self, json: &str) {
        // Malformed or out-of-range events are dropped without being logged: they may be typed text.
        if let Some(bytes) = serde_json::from_str::<InputEvent>(json).ok().and_then(|e| e.encode()) {
            let _ = self.control.send(bytes);
        }
    }
}

#[derive(Default)]
struct VideoState {
    size: (u32, u32),
    config: Option<Vec<u8>>,
}

struct Shared {
    bridge: Bridge,
    sink: Arc<InputSink>,
    video: Mutex<VideoState>,
    opus_head: Mutex<Option<Vec<u8>>>,
    recorder: Mutex<Option<Recorder>>,
    stopping: AtomicBool,
}

struct Active {
    id: u64,
    serial: String,
    shared: Arc<Shared>,
    child: Child,
    port: u16,
    sockets: Vec<TcpStream>,
    threads: Vec<JoinHandle<()>>,
}

pub struct MirrorManager {
    app: AppHandle,
    adb: Arc<Adb>,
    active: Mutex<Option<Active>>,
    status: Mutex<MirrorStatus>,
    recording: Mutex<RecordingStatus>,
    /// Serializes start/stop so a double click cannot race two sessions into existence.
    lifecycle: Mutex<()>,
    next_id: AtomicU64,
}

impl MirrorManager {
    pub fn new(app: AppHandle, adb: Arc<Adb>) -> Arc<Self> {
        Arc::new(Self {
            app,
            adb,
            active: Mutex::new(None),
            status: Mutex::new(MirrorStatus::idle()),
            recording: Mutex::default(),
            lifecycle: Mutex::new(()),
            next_id: AtomicU64::new(1),
        })
    }

    pub fn status(&self) -> MirrorStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn recording_status(&self) -> RecordingStatus {
        self.recording.lock().unwrap().clone()
    }

    pub fn active_serial(&self) -> Option<String> {
        self.active.lock().unwrap().as_ref().map(|a| a.serial.clone())
    }

    fn update(&self, change: impl FnOnce(&mut MirrorStatus)) {
        let snapshot = {
            let mut status = self.status.lock().unwrap();
            change(&mut status);
            status.clone()
        };
        let _ = self.app.emit("mirror-status", snapshot);
    }

    fn update_recording(&self, status: RecordingStatus) {
        *self.recording.lock().unwrap() = status.clone();
        let _ = self.app.emit("recording-status", status);
    }

    pub fn start(self: &Arc<Self>, serial: &str, jar: &Path, options: ServerOptions) -> AppResult<MirrorStatus> {
        validate_serial(serial)?;
        let _guard = self.lifecycle.lock().unwrap();

        let existing = self.active.lock().unwrap().as_ref().map(|a| a.id);
        if let Some(id) = existing {
            self.stop_locked(id, None);
        }

        self.update(|s| {
            *s = MirrorStatus {
                phase: MirrorPhase::Starting,
                serial: Some(serial.to_string()),
                ..MirrorStatus::idle()
            }
        });

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        match self.launch(id, serial, jar, &options) {
            Ok(active) => {
                *self.active.lock().unwrap() = Some(active);
                Ok(self.status())
            }
            Err(e) => {
                self.update(|s| {
                    *s = MirrorStatus { ended_reason: Some(e.to_string()), ..MirrorStatus::idle() }
                });
                Err(e)
            }
        }
    }

    fn launch(self: &Arc<Self>, id: u64, serial: &str, jar: &Path, options: &ServerOptions) -> AppResult<Active> {
        let mut conn = server::launch(&self.adb, serial, jar, options, |step| {
            self.update(|s| s.step = Some(step));
        })?;

        let (control_tx, control_rx) = channel::<Vec<u8>>();
        let sink = Arc::new(InputSink { control: control_tx });
        let bridge = Bridge::start(sink.clone())?;
        let (port, token) = (bridge.port, bridge.token.clone());
        let shared = Arc::new(Shared {
            bridge,
            sink,
            video: Mutex::default(),
            opus_head: Mutex::new(None),
            recorder: Mutex::new(None),
            stopping: AtomicBool::new(false),
        });

        let mut sockets = vec![conn.video.try_clone()?, conn.control.try_clone()?];
        if let Some(a) = &conn.audio {
            sockets.push(a.try_clone()?);
        }

        let mut threads = Vec::new();

        // Control writer: the single owner of writes to the control socket.
        let mut writer = conn.control.try_clone()?;
        threads.push(std::thread::spawn(move || {
            for bytes in control_rx {
                if writer.write_all(&bytes).is_err() {
                    break;
                }
            }
        }));

        // Device messages must be drained or the server's sender eventually blocks.
        let mut reader = conn.control;
        threads.push(std::thread::spawn(move || {
            let mut sink = [0u8; 4096];
            while matches!(reader.read(&mut sink), Ok(n) if n > 0) {}
        }));

        if let Some(audio) = conn.audio.take() {
            let (manager, shared) = (self.clone(), shared.clone());
            threads.push(std::thread::spawn(move || manager.audio_loop(audio, shared)));
        }

        let video = conn.video;
        let (manager, thread_shared) = (self.clone(), shared.clone());
        threads.push(std::thread::spawn(move || manager.video_loop(id, video, thread_shared)));

        let audio_state = if options.audio { AudioState::Pending } else { AudioState::Off };
        let name = conn.device_name.clone();
        self.update(|s| {
            s.phase = MirrorPhase::Running;
            s.step = None;
            s.device_name = Some(name);
            s.audio = audio_state;
            s.port = port;
            s.token = token;
        });
        log::info!("mirroring started on {}", redact(serial));

        Ok(Active {
            id,
            serial: serial.to_string(),
            shared,
            child: conn.child,
            port: conn.port,
            sockets,
            threads,
        })
    }

    fn video_loop(self: Arc<Self>, id: u64, mut socket: TcpStream, shared: Arc<Shared>) {
        let result = self.pump_video(&mut socket, &shared);
        if shared.stopping.load(Ordering::SeqCst) {
            return;
        }
        let reason = match result {
            Err(MirrorEnd::Message(m)) => m,
            _ => "The device was disconnected.".to_string(),
        };
        log::info!("mirroring ended: {reason}");
        // Stopping joins this thread, so it has to happen elsewhere.
        let manager = self.clone();
        std::thread::spawn(move || manager.stop(id, Some(reason)));
    }

    fn pump_video(&self, socket: &mut TcpStream, shared: &Shared) -> Result<(), MirrorEnd> {
        match stream::read_codec_id(socket).map_err(|_| MirrorEnd::Eof)? {
            stream::CODEC_H264 => {}
            stream::CODEC_DISABLED | stream::CODEC_CONFIG_ERROR => {
                return Err(MirrorEnd::Message(
                    "The device could not start its video encoder. Try a lower resolution.".into(),
                ));
            }
            _ => return Err(MirrorEnd::Message("Unexpected video codec from the device.".into())),
        }

        let mut dropping = false;
        loop {
            match stream::read_event(socket).map_err(|_| MirrorEnd::Eof)? {
                StreamEvent::Session { width, height } => {
                    shared.video.lock().unwrap().size = (width, height);
                    if let Some(r) = shared.recorder.lock().unwrap().as_mut() {
                        r.on_session(width, height);
                    }
                    self.update(|s| {
                        s.width = width;
                        s.height = height;
                    });
                }
                StreamEvent::Packet(packet) => {
                    if packet.config {
                        shared.video.lock().unwrap().config = Some(packet.data.clone());
                    }
                    self.record_video(shared, &packet);
                    forward_video(shared, &packet, &mut dropping);
                }
            }
        }
    }

    fn record_video(&self, shared: &Shared, packet: &Packet) {
        let failed = {
            let mut guard = shared.recorder.lock().unwrap();
            let Some(recorder) = guard.as_mut() else { return };
            recorder.on_video(packet);
            recorder.error.is_some().then(|| guard.take()).flatten()
        };
        if let Some(recorder) = failed {
            self.complete_recording(recorder);
        }
    }

    fn audio_loop(self: Arc<Self>, mut socket: TcpStream, shared: Arc<Shared>) {
        let codec = stream::read_codec_id(&mut socket).unwrap_or(stream::CODEC_DISABLED);
        if codec != stream::CODEC_OPUS {
            if let Some(r) = shared.recorder.lock().unwrap().as_mut() {
                r.audio_unavailable();
            }
            self.update(|s| s.audio = AudioState::Unavailable);
            return;
        }
        self.update(|s| s.audio = AudioState::Active);

        while let Ok(StreamEvent::Packet(packet)) = stream::read_event(&mut socket) {
            if packet.config {
                *shared.opus_head.lock().unwrap() = Some(packet.data.clone());
            }
            if let Some(r) = shared.recorder.lock().unwrap().as_mut() {
                r.on_audio(&packet);
            }
            let flags = if packet.config { FLAG_CONFIG } else { 0 };
            // Stale audio is worthless, so a full queue simply drops it.
            shared.bridge.send(frame(KIND_AUDIO, flags, packet.pts_us, &packet.data));
        }
    }

    pub fn stop_active(&self) {
        let _guard = self.lifecycle.lock().unwrap();
        let id = self.active.lock().unwrap().as_ref().map(|a| a.id);
        if let Some(id) = id {
            self.stop_locked(id, None);
        }
    }

    fn stop(&self, id: u64, reason: Option<String>) {
        let _guard = self.lifecycle.lock().unwrap();
        self.stop_locked(id, reason);
    }

    fn stop_locked(&self, id: u64, reason: Option<String>) {
        let active = {
            let mut guard = self.active.lock().unwrap();
            if guard.as_ref().map(|a| a.id) != Some(id) {
                return; // already stopped, or a stale request for an older session
            }
            guard.take()
        };
        let Some(mut active) = active else { return };

        self.update(|s| s.phase = MirrorPhase::Stopping);
        active.shared.stopping.store(true, Ordering::SeqCst);

        // Unblocks every socket thread, then the server exits on its own when its peer goes away.
        for socket in &active.sockets {
            let _ = socket.shutdown(Shutdown::Both);
        }
        let _ = active.child.kill();
        let _ = active.child.wait();
        active.shared.bridge.close();

        // Finalize before any adb call: adb can stall for seconds after a USB loss, and the
        // recording must be safe on disk regardless.
        let recorder = active.shared.recorder.lock().unwrap().take();
        if let Some(recorder) = recorder {
            self.complete_recording(recorder);
        }

        // Dropping the shared sink lets the control writer's channel close.
        let (serial, port) = (active.serial.clone(), active.port);
        let threads = std::mem::take(&mut active.threads);
        drop(active);
        for thread in threads {
            let _ = thread.join();
        }

        self.update(|s| *s = MirrorStatus { ended_reason: reason, ..MirrorStatus::idle() });
        self.adb.remove_forward(&serial, port);
        log::info!("mirroring stopped");
    }

    pub fn start_recording(&self, dir: &Path, base_name: String, audio: bool) -> AppResult<RecordingStatus> {
        std::fs::create_dir_all(dir)?;
        let guard = self.active.lock().unwrap();
        let active = guard.as_ref().ok_or(AppError::NotMirroring)?;
        let shared = &active.shared;

        let mut recorder_slot = shared.recorder.lock().unwrap();
        if recorder_slot.is_some() {
            return Err(AppError::AlreadyRecording);
        }
        let mut recorder = Recorder::new(dir, base_name, audio);
        {
            let video = shared.video.lock().unwrap();
            let head = shared.opus_head.lock().unwrap();
            recorder.prime(video.size, video.config.as_deref(), head.as_deref());
        }
        *recorder_slot = Some(recorder);
        drop(recorder_slot);

        // A recording must begin on a key frame; ask for one instead of waiting for the next.
        shared.sink.request_key_frame();
        drop(guard);

        let status = RecordingStatus {
            active: true,
            started_at_ms: Some(chrono::Utc::now().timestamp_millis()),
            files: Vec::new(),
            error: None,
        };
        self.update_recording(status.clone());
        Ok(status)
    }

    pub fn stop_recording(&self) -> AppResult<RecordingStatus> {
        let recorder = {
            let guard = self.active.lock().unwrap();
            let active = guard.as_ref().ok_or(AppError::NotMirroring)?;
            let recorder = active.shared.recorder.lock().unwrap().take();
            recorder
        };
        if let Some(recorder) = recorder {
            self.complete_recording(recorder);
        }
        Ok(self.recording_status())
    }

    fn complete_recording(&self, recorder: Recorder) {
        let (files, error) = recorder.finish();
        self.update_recording(RecordingStatus {
            active: false,
            started_at_ms: None,
            files: files.iter().map(|p: &PathBuf| p.display().to_string()).collect(),
            error,
        });
    }
}

enum MirrorEnd {
    Eof,
    Message(String),
}

/// Forward to the page without ever blocking the device socket. If the page falls behind, skip
/// ahead: H.264 cannot drop arbitrary frames, so drop to the next key frame and ask for one now.
fn forward_video(shared: &Shared, packet: &Packet, dropping: &mut bool) {
    let mut flags = 0;
    if packet.config {
        flags |= FLAG_CONFIG;
    }
    if packet.key {
        flags |= FLAG_KEY;
    }
    if *dropping {
        if !packet.config && !packet.key {
            return;
        }
        if packet.key {
            // The config that preceded this key frame may have been dropped; resend it first.
            let config = shared.video.lock().unwrap().config.clone();
            if let Some(config) = config {
                shared.bridge.send(frame(KIND_VIDEO, FLAG_CONFIG, 0, &config));
            }
        }
    }
    if shared.bridge.send(frame(KIND_VIDEO, flags, packet.pts_us, &packet.data)) {
        if packet.key {
            *dropping = false;
        }
    } else if !*dropping {
        *dropping = true;
        log::warn!("page fell behind; skipping to the next key frame");
        shared.sink.request_key_frame();
    }
}
