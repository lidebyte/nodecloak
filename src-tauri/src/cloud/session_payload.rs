//! Compressed plaintext exists only in native memory, inside authenticated v2 envelopes.
use super::session_limits::{FRAME_HEADER_BYTES, SNAPSHOT_BYTES, TOO_LARGE};
use flate2::{bufread::GzDecoder, write::GzEncoder, Compression};
use std::io::{Read, Write};
use zeroize::Zeroizing;

const ERROR: &str = "登录快照压缩数据无效，本机数据已保留";
pub fn encode(plain: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
    if plain.len() > SNAPSHOT_BYTES {
        return Err(TOO_LARGE.into());
    }
    let mut gzip = GzEncoder::new(Vec::new(), Compression::fast());
    gzip.write_all(plain).map_err(|_| ERROR)?;
    let compressed = Zeroizing::new(gzip.finish().map_err(|_| ERROR)?);
    let use_gzip = compressed.len() < plain.len();
    let data = if use_gzip {
        compressed.as_slice()
    } else {
        plain
    };
    let mut frame = Zeroizing::new(Vec::with_capacity(FRAME_HEADER_BYTES + data.len()));
    frame.extend_from_slice(b"NCS2");
    frame.push(u8::from(use_gzip));
    frame.extend_from_slice(&(plain.len() as u64).to_le_bytes());
    frame.extend_from_slice(data);
    Ok(frame)
}
pub fn decode(frame: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
    if frame.len() < FRAME_HEADER_BYTES || &frame[..4] != b"NCS2" {
        return Err(ERROR.into());
    }
    let expected = u64::from_le_bytes(frame[5..13].try_into().map_err(|_| ERROR)?);
    if expected > SNAPSHOT_BYTES as u64 {
        return Err(TOO_LARGE.into());
    }
    let data = &frame[FRAME_HEADER_BYTES..];
    match frame[4] {
        0 if data.len() as u64 == expected => Ok(Zeroizing::new(data.to_vec())),
        1 => {
            let mut gzip = GzDecoder::new(data);
            let mut decoded = Zeroizing::new(Vec::new());
            gzip.by_ref()
                .take(expected + 1)
                .read_to_end(&mut decoded)
                .map_err(|_| ERROR)?;
            if decoded.len() as u64 != expected || !gzip.get_ref().is_empty() {
                return Err(ERROR.into());
            }
            Ok(decoded)
        }
        _ => Err(ERROR.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_capacity_roundtrips_and_next_byte_is_rejected() {
        let mut original = vec![b'x'; SNAPSHOT_BYTES];
        let frame = encode(&original).unwrap();
        assert_eq!(decode(&frame).unwrap().as_slice(), original);
        original.push(b'x');
        assert_eq!(encode(&original).unwrap_err(), TOO_LARGE);
    }
    #[test]
    fn frames_reject_expansion_length_mismatches_corruption_and_trailing_members() {
        let original = vec![b'x'; 1 << 20];
        let frame = encode(&original).unwrap();
        assert_eq!(frame[4], 1);
        assert_eq!(decode(&frame).unwrap().as_slice(), original);
        let mut bad = frame.to_vec();
        bad[5..13].copy_from_slice(&((SNAPSHOT_BYTES as u64) + 1).to_le_bytes());
        assert!(decode(&bad).is_err());
        let mut bad = frame.to_vec();
        bad[5..13].copy_from_slice(&10u64.to_le_bytes());
        assert!(decode(&bad).is_err());
        let mut bad = frame.to_vec();
        bad[5..13].copy_from_slice(&((original.len() + 1) as u64).to_le_bytes());
        assert!(decode(&bad).is_err());
        let mut bad = frame.to_vec();
        *bad.last_mut().unwrap() ^= 1;
        assert!(decode(&bad).is_err());
        let mut bad = frame.to_vec();
        bad.push(0);
        assert!(decode(&bad).is_err());
        let mut bad = frame.to_vec();
        bad.extend_from_slice(&frame[FRAME_HEADER_BYTES..]);
        assert!(decode(&bad).is_err());
        assert!(decode(&frame[..frame.len() - 1]).is_err());
        let mut bad = frame.to_vec();
        bad[4] = 2;
        assert!(decode(&bad).is_err());
    }
    #[test]
    fn raw_frames_enforce_exact_length_and_known_magic() {
        let frame = encode(b"x").unwrap();
        assert_eq!(frame[4], 0);
        assert_eq!(decode(&frame).unwrap().as_slice(), b"x");
        for prefix in [0, 4, 12] {
            assert!(decode(&frame[..prefix]).is_err());
        }
        let mut bad = frame.to_vec();
        bad[0] = 0;
        assert!(decode(&bad).is_err());
        let mut bad = frame.to_vec();
        bad.push(0);
        assert!(decode(&bad).is_err());
    }
}
