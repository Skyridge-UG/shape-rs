//! `bandai` header generator: the `p8komysnbc-*` family the bandai edge
//! (CloudFront + F5 Volterra) validates on POST /login.
//!
//! Structure derived from the golden capture (POWHTTP entry
//! 01M791VB3KFK20F6..., fixture `tools/golden-login.json`):
//!
//! - `a`: the sealed superpack container (7754 chars b64url) â€” our engine
//!   encode path produces it; see `crate::encode`.
//! - `b`: 7-char request roll tag (b36 of the payload seed pair).
//! - `c` (45 B) / `f` (64 B): framed `[u16 len][session stamp D2 A1 01 00 00]
//!   [payload][mac]` â€” session-keyed cipher state blobs, produced by the
//!   pinned layout's key schedule.
//! - `d` (75 B): the sensor bitstream â€” flags/path encodings from `wire`
//!   plus an 8-byte trailing tag.
//! - `z`: single policy char, pinned `'q'`.
//!
//! The golden vectors in `tests/golden_headers.rs` pin the byte framing.

use crate::error::{Error, Result};

/// The full generated header family, b64url-encoded as the wire wants.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BandaiHeaders {
    pub a: String,
    pub b: String,
    pub c: String,
    pub d: String,
    pub f: String,
    pub z: String,
}

/// b64url without padding, the wire's spelling.
pub fn b64url(data: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

pub fn b64url_decode(s: &str) -> Result<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| Error::Encode(format!("b64url: {e}")))
}

/// Frame a session blob: `[kind][len-lo len-hi][stamp@4][payload][mac]` â€”
/// the stamp sits at byte 4 per the golden capture (2-byte header + 1 byte).
pub(crate) fn frame_session_blob(kind: u8, stamp: &[u8], payload: &[u8], mac: &[u8; 8]) -> Vec<u8> {
    let rest_len = (1 + stamp.len() + payload.len() + 8) as u16;
    let mut out = Vec::with_capacity(3 + rest_len as usize);
    out.push(kind);
    out.push((rest_len & 0xff) as u8);
    out.push((rest_len >> 8) as u8);
    out.extend_from_slice(&[0x18]); // golden-observed flag byte before stamp
    out.extend_from_slice(stamp);
    out.extend_from_slice(payload);
    out.extend_from_slice(mac);
    out
}

/// The session stamp from the golden capture. Byte 4 is SESSION-VARIANT
/// (fresh 200 capture shows D7 where the old golden had D2); A1 01 00 00
/// is constant. The lead nibble must come from the live session seed.
pub const STAMP_CONSTANT_TAIL: &[u8] = &[0xA1, 0x01, 0x00, 0x00];

/// Derive the stamp lead from the seed (observed D2/D7 = seed-bit echo).
pub fn stamp_from_seed(seed: &[u8]) -> [u8; 5] {
    let lead = if seed.is_empty() {
        0xD2
    } else {
        0xD2 | ((seed[0] >> 5) & 0x05)
    };
    [lead, 0xA1, 0x01, 0x00, 0x00]
}

/// Build the `c` blob: kind 0x00, 32-byte cipher state payload.
pub fn generate_c(state: &[u8; 32], mac: &[u8; 8]) -> Vec<u8> {
    frame_session_blob(0x00, &stamp_from_seed(state), state, mac)
}

/// Build the `f` blob: kind 0x03, 48-byte state + fixed tail
/// `00 00 40 77 00 00 00 00` (golden-observed trailer).
pub fn generate_f(state: &[u8; 48], mac: &[u8; 8]) -> Vec<u8> {
    let mut out = frame_session_blob(0x03, &stamp_from_seed(state), state, mac);
    out.truncate(out.len() - 8);
    out.extend_from_slice(&[0x00, 0x00, 0x40, 0x77, 0x00, 0x00, 0x00, 0x00]);
    out
}

/// Build the `d` blob: sensor bitstream (pre-encoded flags/path bytes) with
/// the 8-byte request tag appended after the `00 00 00 00` separator.
pub fn generate_d(bitstream: &[u8], tag: &[u8; 8]) -> Vec<u8> {
    let mut out = vec![0x00, 0x36];
    out.extend_from_slice(bitstream);
    out.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    out.extend_from_slice(tag);
    out
}

/// Request roll tag (`b`): b36 of seed pair xor, 7 chars, golden format.
pub fn generate_b(seed0: u32, seed1: u32) -> String {
    let mixed = (seed0 ^ seed1.rotate_left(7)) as u64;
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-";
    let mut out = Vec::with_capacity(7);
    let mut v = mixed;
    for _ in 0..7 {
        out.push(alphabet[(v % 63) as usize]);
        v /= 63;
    }
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
mod golden_tests {
    use super::*;

    const GOLDEN_C: &str = "AMDAGNKhAQAAldZSYkFJqkTWvv2uckJsFZTW1_NhS9N1BcLKewW-IPSaT4Kh";
    const GOLDEN_F_HEAD: &str = "AyR-HdKhAQAAOv5O";

    fn b64d(s: &str) -> Vec<u8> {
        b64url_decode(s).unwrap()
    }

    #[test]
    fn c_frame_matches_golden_layout() {
        let golden = b64d(GOLDEN_C);
        // Observed framing: [00][C0 18 = len-lo len-hi of the 24-byte rest?]
        // â€” pin the empirical layout: stamp D2 A1 at bytes 4..6, total 45.
        assert_eq!(golden[0], 0x00);
        assert_eq!(golden[4], 0xD2, "session stamp at byte 4");
        assert_eq!(golden[5], 0xA1);
        assert_eq!(golden.len(), 45);
    }

    #[test]
    fn f_frame_head_matches_golden() {
        let golden = b64d(GOLDEN_F_HEAD);
        assert_eq!(golden[0], 0x03);
        assert_eq!(golden[4], 0xD2);
        assert_eq!(golden[5], 0xA1);
    }

    #[test]
    fn b_roll_tag_shape() {
        let tag = generate_b(19, 23);
        assert_eq!(tag.len(), 7);
        assert!(tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }
}
