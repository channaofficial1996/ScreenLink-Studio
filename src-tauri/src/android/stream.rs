//! Demuxer for the scrcpy v5 video and audio sockets. Pure `Read`-based so it is unit tested
//! without a device. See doc/develop.md#protocol in the scrcpy source.

use std::io::{self, Read};

pub const CODEC_H264: u32 = 0x6832_3634;
pub const CODEC_OPUS: u32 = 0x6F70_7573;
/// The device explicitly disabled the stream (e.g. audio capture unavailable below Android 11).
pub const CODEC_DISABLED: u32 = 0;
/// The device failed to configure the stream.
pub const CODEC_CONFIG_ERROR: u32 = 1;

const FLAG_SESSION: u8 = 0x80;
const FLAG_CONFIG: u64 = 1 << 62;
const FLAG_KEY_FRAME: u64 = 1 << 61;
const PTS_MASK: u64 = FLAG_KEY_FRAME - 1;
/// scrcpy packets are bounded by the encoder's output buffer; anything larger is corruption.
const MAX_PACKET_SIZE: u32 = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub pts_us: u64,
    pub config: bool,
    pub key: bool,
    pub data: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StreamEvent {
    /// A new capture session started; dimensions may differ after a rotation.
    Session { width: u32, height: u32 },
    Packet(Packet),
}

pub fn read_codec_id<R: Read>(r: &mut R) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_be_bytes(buf))
}

pub fn read_event<R: Read>(r: &mut R) -> io::Result<StreamEvent> {
    let mut header = [0u8; 12];
    r.read_exact(&mut header)?;

    if header[0] & FLAG_SESSION != 0 {
        let width = u32::from_be_bytes(header[4..8].try_into().unwrap());
        let height = u32::from_be_bytes(header[8..12].try_into().unwrap());
        if width == 0 || height == 0 {
            return Err(invalid("session packet with an empty video size"));
        }
        return Ok(StreamEvent::Session { width, height });
    }

    let flags = u64::from_be_bytes(header[0..8].try_into().unwrap());
    let size = u32::from_be_bytes(header[8..12].try_into().unwrap());
    if size == 0 || size > MAX_PACKET_SIZE {
        return Err(invalid("media packet with an invalid size"));
    }
    let mut data = vec![0u8; size as usize];
    r.read_exact(&mut data)?;

    let config = flags & FLAG_CONFIG != 0;
    Ok(StreamEvent::Packet(Packet {
        pts_us: if config { 0 } else { flags & PTS_MASK },
        config,
        key: flags & FLAG_KEY_FRAME != 0,
        data,
    }))
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn session(w: u32, h: u32) -> Vec<u8> {
        let mut v = vec![0x80, 0, 0, 0];
        v.extend(w.to_be_bytes());
        v.extend(h.to_be_bytes());
        v
    }

    fn media(flags: u64, payload: &[u8]) -> Vec<u8> {
        let mut v = flags.to_be_bytes().to_vec();
        v.extend((payload.len() as u32).to_be_bytes());
        v.extend(payload);
        v
    }

    #[test]
    fn codec_ids_spell_their_names() {
        assert_eq!(&CODEC_H264.to_be_bytes(), b"h264");
        assert_eq!(&CODEC_OPUS.to_be_bytes(), b"opus");
    }

    #[test]
    fn parses_session_config_key_and_delta_in_order() {
        let mut wire = session(1080, 2400);
        wire.extend(media(FLAG_CONFIG, &[1, 2, 3]));
        wire.extend(media(FLAG_KEY_FRAME | 1_000, &[4, 5]));
        wire.extend(media(17_000, &[6]));
        let mut r = Cursor::new(wire);

        assert_eq!(
            read_event(&mut r).unwrap(),
            StreamEvent::Session { width: 1080, height: 2400 }
        );
        let StreamEvent::Packet(cfg) = read_event(&mut r).unwrap() else { panic!() };
        assert!(cfg.config && !cfg.key);
        assert_eq!(cfg.data, vec![1, 2, 3]);
        let StreamEvent::Packet(key) = read_event(&mut r).unwrap() else { panic!() };
        assert!(key.key && !key.config);
        assert_eq!(key.pts_us, 1_000);
        let StreamEvent::Packet(delta) = read_event(&mut r).unwrap() else { panic!() };
        assert!(!delta.key);
        assert_eq!(delta.pts_us, 17_000);
    }

    #[test]
    fn rejects_corrupt_sizes() {
        assert!(read_event(&mut Cursor::new(media(0, &[]))).is_err());
        let mut huge = 0u64.to_be_bytes().to_vec();
        huge.extend(u32::MAX.to_be_bytes());
        assert!(read_event(&mut Cursor::new(huge)).is_err());
        assert!(read_event(&mut Cursor::new(session(0, 100))).is_err());
    }

    #[test]
    fn truncated_stream_is_an_error_not_a_panic() {
        let mut wire = media(0, &[1, 2, 3, 4]);
        wire.truncate(14);
        assert!(read_event(&mut Cursor::new(wire)).is_err());
    }
}
