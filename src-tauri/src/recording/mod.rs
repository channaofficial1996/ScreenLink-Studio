//! Turns the live stream into MP4 files. A resolution change (the phone rotates) cannot be
//! represented in a single standard avc1 track, so the recording continues in a numbered new file
//! rather than producing a broken one.

pub mod h264;
pub mod mp4;

use crate::android::stream::Packet;
use h264::parse_config;
use mp4::{Mp4Writer, VideoParams};
use std::path::{Path, PathBuf};

pub struct Recorder {
    dir: PathBuf,
    base_name: String,
    part: u32,
    writer: Option<Mp4Writer>,
    size: (u32, u32),
    params: Option<(Vec<u8>, Vec<u8>)>,
    opus_head: Option<Vec<u8>>,
    audio_expected: bool,
    files: Vec<PathBuf>,
    pub error: Option<String>,
}

impl Recorder {
    pub fn new(dir: &Path, base_name: String, audio_expected: bool) -> Self {
        Self {
            dir: dir.to_path_buf(),
            base_name,
            part: 0,
            writer: None,
            size: (0, 0),
            params: None,
            opus_head: None,
            audio_expected,
            files: Vec::new(),
            error: None,
        }
    }

    /// Seed with the current stream state so recording can begin on the next key frame.
    pub fn prime(&mut self, size: (u32, u32), config: Option<&[u8]>, opus_head: Option<&[u8]>) {
        self.size = size;
        self.params = config.and_then(parse_config);
        self.opus_head = opus_head.map(<[u8]>::to_vec);
    }

    pub fn on_session(&mut self, width: u32, height: u32) {
        if self.size != (width, height) {
            self.finish_part();
            self.size = (width, height);
        }
    }

    pub fn on_video(&mut self, packet: &Packet) {
        if self.error.is_some() {
            return;
        }
        if packet.config {
            let params = parse_config(&packet.data);
            if self.writer.is_some() && params != self.params {
                self.finish_part();
            }
            self.params = params;
            return;
        }
        if self.writer.is_none() {
            if !packet.key {
                return;
            }
            self.open_part();
        }
        if let Some(writer) = &mut self.writer {
            if let Err(e) = writer.write_video(packet.pts_us, packet.key, &packet.data) {
                self.fail(e);
            }
        }
    }

    pub fn on_audio(&mut self, packet: &Packet) {
        if packet.config {
            self.opus_head = Some(packet.data.clone());
            return;
        }
        if let Some(writer) = &mut self.writer {
            if let Err(e) = writer.write_audio(packet.pts_us, &packet.data) {
                self.fail(e);
            }
        }
    }

    /// The device reported that it cannot capture audio.
    pub fn audio_unavailable(&mut self) {
        self.audio_expected = false;
    }

    fn open_part(&mut self) {
        let Some((sps, pps)) = self.params.clone() else { return };
        self.part += 1;
        let name = if self.part == 1 {
            format!("{}.mp4", self.base_name)
        } else {
            format!("{} (part {}).mp4", self.base_name, self.part)
        };
        let params = VideoParams { width: self.size.0, height: self.size.1, sps, pps };
        let audio = self.opus_head.clone().filter(|_| self.audio_expected);
        match Mp4Writer::create(&self.dir.join(name), params, audio) {
            Ok(w) => self.writer = Some(w),
            Err(e) => self.fail(e),
        }
    }

    fn finish_part(&mut self) {
        if let Some(writer) = self.writer.take() {
            match writer.finish() {
                Ok(path) => self.files.push(path),
                Err(e) => self.fail(e),
            }
        }
    }

    fn fail(&mut self, e: std::io::Error) {
        log::error!("recording failed: {e}");
        self.error = Some(format!("Recording stopped: {e}"));
        self.writer = None;
    }

    pub fn finish(mut self) -> (Vec<PathBuf>, Option<String>) {
        self.finish_part();
        (self.files, self.error)
    }
}
