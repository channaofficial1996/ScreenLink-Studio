//! Tests against a real phone. Ignored by default; run with a device attached:
//!   cargo test --lib hardware -- --ignored --nocapture

use super::adb::{Adb, AdbState};
use super::control::InputEvent;
use super::server::{self, ServerOptions};
use super::stream::{self, StreamEvent};
use crate::recording::Recorder;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn adb_and_serial() -> (Adb, String) {
    let adb = Adb::locate(None).expect("adb");
    adb.start_server().expect("adb server");
    let serial = adb
        .devices()
        .expect("devices")
        .into_iter()
        .find(|d| d.state == AdbState::Device)
        .expect("an authorized device must be connected")
        .serial;
    (adb, serial)
}

fn jar() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").join("scrcpy-server")
}

fn options() -> ServerOptions {
    ServerOptions {
        max_size: 1280,
        max_fps: 30,
        video_bit_rate: 4_000_000,
        audio: true,
        stay_awake: false,
        show_touches: false,
    }
}

#[test]
#[ignore]
fn hardware_reads_device_properties() {
    let (adb, serial) = adb_and_serial();
    let p = adb.props(&serial).unwrap();
    println!("{p:?}");
    assert!(p.model.is_some());
    assert!(p.sdk.unwrap() >= 21);
}

#[test]
#[ignore]
fn hardware_screenshot_is_a_png() {
    let (adb, serial) = adb_and_serial();
    let png = adb.screenshot(&serial).unwrap();
    println!("screenshot: {} bytes", png.len());
    assert!(png.len() > 1000);
}

#[test]
#[ignore]
fn hardware_stream_control_and_record() {
    let (adb, serial) = adb_and_serial();
    let conn = server::launch(&adb, &serial, &jar(), &options(), |s| println!("step: {s}")).unwrap();
    println!("device name: {:?}", conn.device_name);
    assert!(!conn.device_name.is_empty());
    let mut child = conn.child;

    let dir = std::env::temp_dir().join(format!("slstudio-hw-{}", rand::random::<u32>()));
    std::fs::create_dir_all(&dir).unwrap();
    let recorder = Arc::new(Mutex::new(Recorder::new(&dir, "hw".into(), true)));

    // Audio on its own thread, as in the app.
    let done = Arc::new(AtomicBool::new(false));
    let mut audio = conn.audio.unwrap();
    let audio_recorder = recorder.clone();
    let audio_packets = Arc::new(Mutex::new(0u32));
    let audio_count = audio_packets.clone();
    let audio_thread = std::thread::spawn(move || {
        let codec = stream::read_codec_id(&mut audio).unwrap();
        println!("audio codec id: {codec:#x}");
        if codec != stream::CODEC_OPUS {
            return false;
        }
        while let Ok(StreamEvent::Packet(p)) = stream::read_event(&mut audio) {
            if p.config {
                println!("opus config ({} bytes): {:?}", p.data.len(), &p.data[..8.min(p.data.len())]);
                assert!(p.data.starts_with(b"OpusHead"));
            }
            audio_recorder.lock().unwrap().on_audio(&p);
            *audio_count.lock().unwrap() += 1;
        }
        true
    });

    let mut video = conn.video;
    assert_eq!(stream::read_codec_id(&mut video).unwrap(), stream::CODEC_H264);

    // Injects HOME then a swipe so the screen changes and frames keep arriving.
    let mut control = conn.control.try_clone().unwrap();
    let controller = std::thread::spawn({
        let done = done.clone();
        move || {
            std::thread::sleep(Duration::from_millis(1000));
            for action in [0, 1] {
                let e = InputEvent::Key { action, code: 3, repeat: 0, meta: 0 };
                control.write_all(&e.encode().unwrap()).unwrap();
            }
            std::thread::sleep(Duration::from_millis(500));
            while !done.load(Ordering::SeqCst) {
                let reset = InputEvent::ExpandNotifications.encode().unwrap();
                let _ = control.write_all(&reset);
                std::thread::sleep(Duration::from_millis(700));
                let reset = InputEvent::CollapsePanels.encode().unwrap();
                let _ = control.write_all(&reset);
                std::thread::sleep(Duration::from_millis(700));
            }
        }
    });

    let (mut sessions, mut config, mut keys, mut frames, mut bytes) = (0, 0, 0, 0u32, 0usize);
    let start = Instant::now();
    video.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    while start.elapsed() < Duration::from_secs(6) {
        match stream::read_event(&mut video).expect("video stream") {
            StreamEvent::Session { width, height } => {
                println!("session {width}x{height}");
                sessions += 1;
                recorder.lock().unwrap().on_session(width, height);
            }
            StreamEvent::Packet(p) => {
                if p.config {
                    config += 1;
                } else {
                    frames += 1;
                    keys += p.key as u32;
                }
                bytes += p.data.len();
                recorder.lock().unwrap().on_video(&p);
            }
        }
    }
    println!("sessions={sessions} config={config} keys={keys} frames={frames} bytes={bytes} audio={}",
        audio_packets.lock().unwrap());

    done.store(true, Ordering::SeqCst);
    controller.join().unwrap();
    let _ = conn.control.shutdown(std::net::Shutdown::Both);
    let _ = video.shutdown(std::net::Shutdown::Both);
    let _ = child.kill();
    let _ = child.wait();
    adb.remove_forward(&serial, conn.port);
    let audio_ok = audio_thread.join().unwrap();

    assert!(sessions >= 1 && config >= 1 && keys >= 1);
    assert!(frames > 10, "only {frames} frames arrived");

    let recorder = Arc::try_unwrap(recorder).ok().unwrap().into_inner().unwrap();
    let (files, error) = recorder.finish();
    assert_eq!(error, None);
    assert_eq!(files.len(), 1, "{files:?}");
    let keep = std::env::temp_dir().join("slstudio-hw-latest.mp4");
    std::fs::copy(&files[0], &keep).unwrap();
    println!("recording: {} (audio stream ok: {audio_ok})", keep.display());

    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries",
            "stream=codec_name,width,height,nb_read_frames,duration:format=duration",
            "-count_frames", "-of", "default=nw=1"])
        .arg(&keep)
        .output()
        .expect("ffprobe");
    let report = String::from_utf8_lossy(&probe.stdout);
    println!("{report}{}", String::from_utf8_lossy(&probe.stderr));
    assert!(probe.status.success());
    assert!(report.contains("codec_name=h264"));
    assert!(report.contains("codec_name=opus"), "the recording has no audio track");

    let decode = Command::new("ffmpeg")
        .args(["-v", "error", "-xerror", "-i"])
        .arg(&keep)
        .args(["-fps_mode", "passthrough", "-f", "null", "-"])
        .output()
        .expect("ffmpeg");
    println!("decode errors: {}", String::from_utf8_lossy(&decode.stderr));
    assert!(decode.status.success(), "the recording does not fully decode");
    std::fs::remove_dir_all(&dir).unwrap();
}
