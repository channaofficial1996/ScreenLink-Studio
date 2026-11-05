//! Minimal MP4 writer for one H.264 track and an optional Opus track.
//!
//! Recording muxes the encoded stream directly, so it costs no CPU and has no quality loss
//! compared with what the device produced. Samples are written as they arrive to a `.part` file;
//! the `moov` index goes at the end and the file is renamed only when complete.

use super::h264::{annexb_to_avcc, avc_decoder_config};
use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const VIDEO_TIMESCALE: u32 = 90_000;
const AUDIO_TIMESCALE: u32 = 48_000;
const MOVIE_TIMESCALE: u32 = 1_000;
const OPUS_FRAME_TICKS: u32 = 960;
const DEFAULT_VIDEO_TICKS: u32 = VIDEO_TIMESCALE / 60;

pub struct VideoParams {
    pub width: u32,
    pub height: u32,
    pub sps: Vec<u8>,
    pub pps: Vec<u8>,
}

#[derive(Default)]
struct Track {
    sizes: Vec<u32>,
    offsets: Vec<u64>,
    deltas: Vec<u32>,
    first_pts: Option<u64>,
    last_ticks: u64,
}

impl Track {
    fn push(&mut self, pts_us: u64, size: u32, offset: u64, timescale: u32) {
        let first = *self.first_pts.get_or_insert(pts_us);
        let ticks = (pts_us.saturating_sub(first) as u128 * timescale as u128 / 1_000_000) as u64;
        if !self.sizes.is_empty() {
            let delta = ticks.saturating_sub(self.last_ticks).clamp(1, u32::MAX as u64);
            self.deltas.push(delta as u32);
            self.last_ticks += delta;
        }
        self.sizes.push(size);
        self.offsets.push(offset);
    }

    fn close(&mut self, default_delta: u32) {
        if !self.sizes.is_empty() {
            let delta = self.deltas.last().copied().unwrap_or(default_delta);
            self.deltas.push(delta);
        }
    }

    fn duration(&self) -> u64 {
        self.deltas.iter().map(|d| *d as u64).sum()
    }
}

pub struct Mp4Writer {
    out: BufWriter<File>,
    part_path: PathBuf,
    final_path: PathBuf,
    video_params: VideoParams,
    opus_head: Option<Vec<u8>>,
    mdat_start: u64,
    position: u64,
    video: Track,
    audio: Track,
    key_frames: Vec<u32>,
    scratch: Vec<u8>,
}

impl Mp4Writer {
    pub fn create(path: &Path, video: VideoParams, opus_head: Option<Vec<u8>>) -> io::Result<Self> {
        let part_path = path.with_extension("mp4.part");
        let mut out = BufWriter::with_capacity(1 << 20, File::create(&part_path)?);

        let mut ftyp = Vec::new();
        ftyp.extend(b"isom");
        ftyp.extend(512u32.to_be_bytes());
        ftyp.extend(b"isomiso2avc1mp41");
        let ftyp = boxed(b"ftyp", &ftyp);
        out.write_all(&ftyp)?;

        // 64-bit mdat so recordings can exceed 4 GiB; the size is patched in finish().
        let mdat_start = ftyp.len() as u64;
        out.write_all(&1u32.to_be_bytes())?;
        out.write_all(b"mdat")?;
        out.write_all(&0u64.to_be_bytes())?;

        Ok(Self {
            out,
            part_path,
            final_path: path.to_path_buf(),
            video_params: video,
            opus_head,
            mdat_start,
            position: mdat_start + 16,
            video: Track::default(),
            audio: Track::default(),
            key_frames: Vec::new(),
            scratch: Vec::new(),
        })
    }

    pub fn write_video(&mut self, pts_us: u64, key: bool, annexb: &[u8]) -> io::Result<()> {
        self.scratch.clear();
        annexb_to_avcc(annexb, &mut self.scratch);
        if self.scratch.is_empty() {
            return Ok(());
        }
        if key {
            self.key_frames.push(self.video.sizes.len() as u32 + 1);
        }
        let size = self.scratch.len() as u32;
        self.video.push(pts_us, size, self.position, VIDEO_TIMESCALE);
        self.out.write_all(&self.scratch)?;
        self.position += size as u64;
        Ok(())
    }

    pub fn write_audio(&mut self, pts_us: u64, data: &[u8]) -> io::Result<()> {
        if self.opus_head.is_none() {
            return Ok(());
        }
        self.audio.push(pts_us, data.len() as u32, self.position, AUDIO_TIMESCALE);
        self.out.write_all(data)?;
        self.position += data.len() as u64;
        Ok(())
    }

    /// Write the index, patch the mdat size, and atomically publish the finished file.
    pub fn finish(mut self) -> io::Result<PathBuf> {
        self.video.close(DEFAULT_VIDEO_TICKS);
        self.audio.close(OPUS_FRAME_TICKS);

        let moov = self.build_moov();
        let mdat_size = self.position - self.mdat_start;
        self.out.write_all(&moov)?;
        self.out.flush()?;

        let file = self.out.get_mut();
        file.seek(SeekFrom::Start(self.mdat_start + 8))?;
        file.write_all(&mdat_size.to_be_bytes())?;
        file.sync_all()?;
        drop(self.out);

        std::fs::rename(&self.part_path, &self.final_path)?;
        Ok(self.final_path)
    }

    fn build_moov(&self) -> Vec<u8> {
        let has_audio = self.opus_head.is_some() && !self.audio.sizes.is_empty();
        let v_first = self.video.first_pts.unwrap_or(0);
        let a_first = self.audio.first_pts.unwrap_or(v_first);
        // Both tracks share the device clock; delay whichever started later to keep them in sync.
        let origin = if has_audio { v_first.min(a_first) } else { v_first };
        let v_delay = us_to_movie(v_first - origin);
        let a_delay = us_to_movie(a_first.saturating_sub(origin));

        let v_duration = scale(self.video.duration(), VIDEO_TIMESCALE) + v_delay;
        let a_duration = scale(self.audio.duration(), AUDIO_TIMESCALE) + a_delay;
        let duration = if has_audio { v_duration.max(a_duration) } else { v_duration };

        let mut moov = mvhd(duration, if has_audio { 3 } else { 2 });
        moov.extend(self.video_trak(v_delay));
        if has_audio {
            moov.extend(self.audio_trak(a_delay));
        }
        boxed(b"moov", &moov)
    }

    fn video_trak(&self, delay: u64) -> Vec<u8> {
        let t = &self.video;
        let (w, h) = (self.video_params.width, self.video_params.height);
        let avcc = boxed(b"avcC", &avc_decoder_config(&self.video_params.sps, &self.video_params.pps));

        let mut entry = vec![0u8; 6];
        entry.extend(1u16.to_be_bytes()); // data reference index
        entry.extend([0u8; 16]); // pre-defined / reserved
        entry.extend((w as u16).to_be_bytes());
        entry.extend((h as u16).to_be_bytes());
        entry.extend(0x0048_0000u32.to_be_bytes()); // 72 dpi
        entry.extend(0x0048_0000u32.to_be_bytes());
        entry.extend([0u8; 4]);
        entry.extend(1u16.to_be_bytes()); // frames per sample
        entry.extend([0u8; 32]); // compressor name
        entry.extend(0x0018u16.to_be_bytes()); // depth
        entry.extend((-1i16).to_be_bytes());
        entry.extend(&avcc);
        let stsd = stsd(&boxed(b"avc1", &entry));

        let mut stbl = stsd;
        stbl.extend(stts(&t.deltas));
        stbl.extend(stss(&self.key_frames));
        stbl.extend(sample_tables(t));

        let minf = media_info(&boxed_full(b"vmhd", 0, 1, &[0u8; 8]), &stbl);
        let mdia = mdia(VIDEO_TIMESCALE, t.duration(), b"vide", b"ScreenLink Video\0", &minf);
        trak(1, scale(t.duration(), VIDEO_TIMESCALE), w, h, 0, delay, &mdia)
    }

    fn audio_trak(&self, delay: u64) -> Vec<u8> {
        let t = &self.audio;
        let head = self.opus_head.as_deref().unwrap_or_default();

        let mut entry = vec![0u8; 6];
        entry.extend(1u16.to_be_bytes());
        entry.extend([0u8; 8]);
        entry.extend(2u16.to_be_bytes()); // channels
        entry.extend(16u16.to_be_bytes()); // sample size
        entry.extend([0u8; 4]);
        entry.extend((AUDIO_TIMESCALE << 16).to_be_bytes());
        entry.extend(boxed(b"dOps", &opus_specific(head)));
        let stsd = stsd(&boxed(b"Opus", &entry));

        let mut stbl = stsd;
        stbl.extend(stts(&t.deltas));
        stbl.extend(sample_tables(t));

        let minf = media_info(&boxed_full(b"smhd", 0, 0, &[0u8; 4]), &stbl);
        let mdia = mdia(AUDIO_TIMESCALE, t.duration(), b"soun", b"ScreenLink Audio\0", &minf);
        trak(2, scale(t.duration(), AUDIO_TIMESCALE), 0, 0, 0x0100, delay, &mdia)
    }
}

fn scale(ticks: u64, timescale: u32) -> u64 {
    ticks * MOVIE_TIMESCALE as u64 / timescale as u64
}

fn us_to_movie(us: u64) -> u64 {
    us * MOVIE_TIMESCALE as u64 / 1_000_000
}

fn boxed(name: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend((8 + payload.len() as u32).to_be_bytes());
    out.extend(name);
    out.extend_from_slice(payload);
    out
}

fn boxed_full(name: &[u8; 4], version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + payload.len());
    body.push(version);
    body.extend(&flags.to_be_bytes()[1..]);
    body.extend_from_slice(payload);
    boxed(name, &body)
}

const IDENTITY_MATRIX: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];

fn mvhd(duration: u64, next_track: u32) -> Vec<u8> {
    let mut b = vec![0u8; 16]; // creation, modification (64-bit each)
    b.extend(MOVIE_TIMESCALE.to_be_bytes());
    b.extend(duration.to_be_bytes());
    b.extend(0x0001_0000u32.to_be_bytes()); // rate
    b.extend(0x0100u16.to_be_bytes()); // volume
    b.extend([0u8; 10]);
    for v in IDENTITY_MATRIX {
        b.extend(v.to_be_bytes());
    }
    b.extend([0u8; 24]);
    b.extend(next_track.to_be_bytes());
    boxed_full(b"mvhd", 1, 0, &b)
}

fn trak(id: u32, duration: u64, w: u32, h: u32, volume: u16, delay: u64, mdia: &[u8]) -> Vec<u8> {
    let mut t = vec![0u8; 16];
    t.extend(id.to_be_bytes());
    t.extend([0u8; 4]);
    t.extend(duration.to_be_bytes());
    t.extend([0u8; 12]);
    t.extend(volume.to_be_bytes());
    t.extend([0u8; 2]);
    for v in IDENTITY_MATRIX {
        t.extend(v.to_be_bytes());
    }
    t.extend((w << 16).to_be_bytes());
    t.extend((h << 16).to_be_bytes());
    let mut body = boxed_full(b"tkhd", 1, 3, &t); // enabled | in movie

    if delay > 0 {
        // Empty edit: the track begins `delay` ms into the movie.
        let mut elst = 2u32.to_be_bytes().to_vec();
        elst.extend(delay.to_be_bytes());
        elst.extend((-1i64).to_be_bytes());
        elst.extend(0x0001_0000u32.to_be_bytes());
        elst.extend(duration.to_be_bytes());
        elst.extend(0i64.to_be_bytes());
        elst.extend(0x0001_0000u32.to_be_bytes());
        body.extend(boxed(b"edts", &boxed_full(b"elst", 1, 0, &elst)));
    }
    body.extend_from_slice(mdia);
    boxed(b"trak", &body)
}

fn mdia(timescale: u32, duration: u64, handler: &[u8; 4], name: &[u8], minf: &[u8]) -> Vec<u8> {
    let mut mdhd = vec![0u8; 16];
    mdhd.extend(timescale.to_be_bytes());
    mdhd.extend(duration.to_be_bytes());
    mdhd.extend(0x55c4u16.to_be_bytes()); // language: und
    mdhd.extend([0u8; 2]);
    let mut body = boxed_full(b"mdhd", 1, 0, &mdhd);

    let mut hdlr = vec![0u8; 4];
    hdlr.extend(handler);
    hdlr.extend([0u8; 12]);
    hdlr.extend_from_slice(name);
    body.extend(boxed_full(b"hdlr", 0, 0, &hdlr));
    body.extend_from_slice(minf);
    boxed(b"mdia", &body)
}

fn media_info(header: &[u8], stbl: &[u8]) -> Vec<u8> {
    let url = boxed_full(b"url ", 0, 1, &[]);
    let mut dref = 1u32.to_be_bytes().to_vec();
    dref.extend(url);
    let dinf = boxed(b"dinf", &boxed_full(b"dref", 0, 0, &dref));
    let mut body = header.to_vec();
    body.extend(dinf);
    body.extend(boxed(b"stbl", stbl));
    boxed(b"minf", &body)
}

fn stsd(entry: &[u8]) -> Vec<u8> {
    let mut b = 1u32.to_be_bytes().to_vec();
    b.extend_from_slice(entry);
    boxed_full(b"stsd", 0, 0, &b)
}

/// Run-length encode sample durations.
fn stts(deltas: &[u32]) -> Vec<u8> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for d in deltas {
        match runs.last_mut() {
            Some((count, delta)) if delta == d => *count += 1,
            _ => runs.push((1, *d)),
        }
    }
    let mut b = (runs.len() as u32).to_be_bytes().to_vec();
    for (count, delta) in runs {
        b.extend(count.to_be_bytes());
        b.extend(delta.to_be_bytes());
    }
    boxed_full(b"stts", 0, 0, &b)
}

fn stss(key_frames: &[u32]) -> Vec<u8> {
    let mut b = (key_frames.len() as u32).to_be_bytes().to_vec();
    for k in key_frames {
        b.extend(k.to_be_bytes());
    }
    boxed_full(b"stss", 0, 0, &b)
}

/// stsc, stsz and co64. Every sample is its own chunk, which handles interleaved tracks simply.
fn sample_tables(t: &Track) -> Vec<u8> {
    let mut stsc = 1u32.to_be_bytes().to_vec();
    stsc.extend(1u32.to_be_bytes());
    stsc.extend(1u32.to_be_bytes());
    stsc.extend(1u32.to_be_bytes());
    let mut out = boxed_full(b"stsc", 0, 0, &stsc);

    let mut stsz = 0u32.to_be_bytes().to_vec();
    stsz.extend((t.sizes.len() as u32).to_be_bytes());
    for s in &t.sizes {
        stsz.extend(s.to_be_bytes());
    }
    out.extend(boxed_full(b"stsz", 0, 0, &stsz));

    let mut co64 = (t.offsets.len() as u32).to_be_bytes().to_vec();
    for o in &t.offsets {
        co64.extend(o.to_be_bytes());
    }
    out.extend(boxed_full(b"co64", 0, 0, &co64));
    out
}

/// Convert a little-endian OpusHead into the big-endian dOps payload (ISO Opus-in-MP4 mapping).
fn opus_specific(head: &[u8]) -> Vec<u8> {
    let le16 = |i: usize| head.get(i..i + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
    let le32 = |i: usize| {
        head.get(i..i + 4)
            .map_or(48_000, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let mut b = vec![0u8]; // version
    b.push(head.get(9).copied().unwrap_or(2)); // output channels
    b.extend(le16(10).to_be_bytes()); // pre-skip
    b.extend(le32(12).to_be_bytes()); // input sample rate
    b.extend((le16(16) as i16).to_be_bytes()); // output gain
    b.push(0); // channel mapping family
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_head_converts_to_dops() {
        let head = [
            b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd', 1, 2, 0x38, 0x01, 0x80, 0xbb, 0, 0, 0,
            0, 0,
        ];
        assert_eq!(opus_specific(&head), [0, 2, 0x01, 0x38, 0, 0, 0xbb, 0x80, 0, 0, 0]);
    }

    #[test]
    fn stts_run_length_encodes() {
        let b = stts(&[10, 10, 10, 20]);
        // size(4) stts(4) fullbox(4) count(4) then two runs
        assert_eq!(&b[12..16], &2u32.to_be_bytes());
        assert_eq!(&b[16..24], &[0, 0, 0, 3, 0, 0, 0, 10]);
        assert_eq!(&b[24..32], &[0, 0, 0, 1, 0, 0, 0, 20]);
    }

    #[test]
    fn track_timing_uses_presentation_timestamps() {
        let mut t = Track::default();
        t.push(1_000_000, 10, 0, 90_000);
        t.push(1_016_667, 10, 10, 90_000);
        t.push(1_050_000, 10, 20, 90_000);
        t.close(1500);
        assert_eq!(t.deltas, vec![1500, 3000, 3000]);
    }

    #[test]
    fn non_monotonic_pts_never_produces_zero_delta() {
        let mut t = Track::default();
        t.push(1_000, 1, 0, 90_000);
        t.push(1_000, 1, 1, 90_000);
        t.close(1500);
        assert!(t.deltas.iter().all(|d| *d >= 1));
    }
}
