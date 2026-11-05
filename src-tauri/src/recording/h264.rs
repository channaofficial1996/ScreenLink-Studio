//! H.264 bitstream helpers for MP4 muxing.

pub const NAL_SPS: u8 = 7;
pub const NAL_PPS: u8 = 8;

/// Iterate NAL units in an Annex B byte stream (start codes removed).
pub fn nal_units(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let ends: Vec<usize> = starts
        .iter()
        .skip(1)
        .map(|s| s - 3)
        .chain(std::iter::once(data.len()))
        .collect();
    starts.into_iter().zip(ends).map(move |(s, e)| {
        let mut end = e;
        // Drop trailing zeros (the first byte of a 4-byte start code).
        while end > s && data[end - 1] == 0 {
            end -= 1;
        }
        &data[s..end]
    })
}

pub fn nal_type(nal: &[u8]) -> u8 {
    nal.first().map_or(0, |b| b & 0x1f)
}

/// Extract the parameter sets from a scrcpy config packet.
pub fn parse_config(config: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut sps = None;
    let mut pps = None;
    for nal in nal_units(config) {
        match nal_type(nal) {
            NAL_SPS if sps.is_none() => sps = Some(nal.to_vec()),
            NAL_PPS if pps.is_none() => pps = Some(nal.to_vec()),
            _ => {}
        }
    }
    Some((sps?, pps?))
}

/// Length-prefix each NAL. Parameter sets are omitted: they live in the avcC box, and a mid-file
/// change starts a new file instead.
pub fn annexb_to_avcc(data: &[u8], out: &mut Vec<u8>) {
    for nal in nal_units(data) {
        if nal.is_empty() || matches!(nal_type(nal), NAL_SPS | NAL_PPS) {
            continue;
        }
        out.extend((nal.len() as u32).to_be_bytes());
        out.extend_from_slice(nal);
    }
}

/// AVCDecoderConfigurationRecord for the avcC box.
pub fn avc_decoder_config(sps: &[u8], pps: &[u8]) -> Vec<u8> {
    let mut b = vec![
        1,
        sps.get(1).copied().unwrap_or(66),
        sps.get(2).copied().unwrap_or(0),
        sps.get(3).copied().unwrap_or(31),
        0xff, // 4-byte NAL lengths
        0xe1, // one SPS
    ];
    b.extend((sps.len() as u16).to_be_bytes());
    b.extend_from_slice(sps);
    b.push(1); // one PPS
    b.extend((pps.len() as u16).to_be_bytes());
    b.extend_from_slice(pps);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &[u8] = &[0, 0, 0, 1, 0x67, 0x42, 0xc0, 0x1f, 0, 0, 0, 1, 0x68, 0xce, 0x38, 0x80];

    #[test]
    fn splits_three_and_four_byte_start_codes() {
        let data = [0, 0, 0, 1, 0x67, 1, 0, 0, 1, 0x68, 2, 0, 0, 0, 1, 0x65, 3];
        let nals: Vec<&[u8]> = nal_units(&data).collect();
        assert_eq!(nals, vec![&[0x67, 1][..], &[0x68, 2][..], &[0x65, 3][..]]);
    }

    #[test]
    fn extracts_parameter_sets() {
        let (sps, pps) = parse_config(CONFIG).unwrap();
        assert_eq!(sps, [0x67, 0x42, 0xc0, 0x1f]);
        assert_eq!(pps, [0x68, 0xce, 0x38, 0x80]);
        assert!(parse_config(&[0, 0, 1, 0x65, 1]).is_none());
    }

    #[test]
    fn converts_to_length_prefixed_and_strips_parameter_sets() {
        let mut out = Vec::new();
        let frame = [0, 0, 0, 1, 0x67, 9, 0, 0, 0, 1, 0x65, 1, 2, 3];
        annexb_to_avcc(&frame, &mut out);
        assert_eq!(out, [0, 0, 0, 4, 0x65, 1, 2, 3]);
    }

    #[test]
    fn decoder_config_layout() {
        let (sps, pps) = parse_config(CONFIG).unwrap();
        let c = avc_decoder_config(&sps, &pps);
        assert_eq!(&c[..6], &[1, 0x42, 0xc0, 0x1f, 0xff, 0xe1]);
        assert_eq!(&c[6..8], &[0, 4]);
    }
}
