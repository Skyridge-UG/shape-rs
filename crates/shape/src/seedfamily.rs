//! Seed->container: derive the p8komysnbc family from the per-response
//! seed blob (recon 'seed mechanism'). The seed IS the engine layout state
//! in custom-alphabet b64; this module bridges seed bytes -> cipher state
//! -> family headers, and the a-carrier when the engine feature is on.

use crate::bandai;
use crate::error::{Error, Result};

/// The bandai custom alphabet (recon 'bandai alphabet extracted').
pub const BANDAI_ALPHABET: &str =
    "-0123456789=ABCDEFGHIJKLMNOPQRSTUVWXYZ_acdefghijklmnopqrstuvwxyz";

/// Custom-b64 decode (6-bit big-endian pack, per the engine symbol table).
pub fn seed_decode(text: &str) -> Vec<u8> {
    let map: std::collections::HashMap<char, u8> = BANDAI_ALPHABET
        .chars()
        .enumerate()
        .map(|(i, c)| (c, i as u8))
        .collect();
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for ch in text.chars() {
        if let Some(v) = map.get(&ch) {
            acc = (acc << 6) | u32::from(*v);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push(((acc >> bits) & 0xff) as u8);
            }
        }
    }
    out
}

/// Custom-b64 encode (inverse).
pub fn seed_encode(bytes: &[u8]) -> String {
    let chars: Vec<char> = BANDAI_ALPHABET.chars().collect();
    let mut out = String::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for b in bytes {
        acc = (acc << 8) | u32::from(*b);
        bits += 8;
        while bits >= 6 {
            bits -= 6;
            out.push(chars[((acc >> bits) & 0x3f) as usize]);
        }
    }
    if bits > 0 {
        out.push(chars[((acc << (6 - bits)) & 0x3f) as usize]);
    }
    out
}

/// The family header set derived from a page seed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FamilyHeaders {
    pub family: String,
    pub a: String,
    pub b: String,
    pub c: String,
    pub d: String,
    pub f: String,
    pub z: String,
}

/// Generate the full family from the per-response seed + request seeds.
///
/// seed layout (45 B observed): the leading bytes are the cipher iv/state
/// seed; the trailing bytes the per-session keys. The split mirrors the
/// engine Session (4 iv words + 8 key words = 48 B pool; the seed provides
/// 45 and the remainder pads from the request seed).
pub fn generate_from_seed(
    family: &str,
    seed_b64: &str,
    request_seed: u32,
) -> Result<FamilyHeaders> {
    let seed = seed_decode(seed_b64);
    if seed.len() < 32 {
        return Err(Error::Encode(format!(
            "seed too short: {} bytes",
            seed.len()
        )));
    }
    // iv/state words from the seed head; mac from the tail.
    let mut state_c = [0u8; 32];
    state_c.copy_from_slice(&seed[..32]);
    let mut mac = [0u8; 8];
    let tail = &seed[32.min(seed.len())..];
    for (i, b) in mac.iter_mut().enumerate() {
        *b = tail
            .get(i)
            .copied()
            .unwrap_or((request_seed >> (i % 4 * 8)) as u8);
    }

    let mut state_f = [0u8; 48];
    for (i, b) in state_f.iter_mut().enumerate() {
        *b = seed.get(i % seed.len()).copied().unwrap_or(0) ^ ((i as u8).wrapping_mul(13));
    }

    let c = bandai::generate_c(&state_c, &mac);
    let f = bandai::generate_f(&state_f, &mac);
    let d = bandai::generate_d(&seed[8.min(seed.len())..], &mac);

    Ok(FamilyHeaders {
        family: family.to_owned(),
        // a-carrier: the sealed container; without the engine the framed
        // state stands in (structurally valid, c-carrier form).
        #[cfg(feature = "engine")]
        a: String::new(), // wired by caller via encode_headers
        #[cfg(not(feature = "engine"))]
        a: bandai::b64url(&c),
        b: bandai::generate_b(
            request_seed,
            u32::from_le_bytes([seed[0], seed[1], seed[2], seed[3]]),
        ),
        c: bandai::b64url(&c),
        d: bandai::b64url(&d),
        f: bandai::b64url(&f),
        z: "q".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE_SEED: &str = "AIAO2tWhAQAAhKju6Z7JSplIS50BU294mrM3QyG0M37--mk9UlrfhoAK2Nhj";

    #[test]
    fn seed_roundtrip() {
        let bytes = seed_decode(PAGE_SEED);
        assert_eq!(bytes.len(), 45);
        assert_eq!(seed_encode(&bytes), PAGE_SEED, "custom-b64 inverse");
    }

    #[test]
    fn family_from_live_seed_is_wellformed() {
        let fam = generate_from_seed("p8kOmysnbc", PAGE_SEED, 19).unwrap();
        assert_eq!(fam.family, "p8kOmysnbc");
        assert_eq!(
            fam.c.len(),
            66,
            "50B framed blob b64url (48 state + stamp pad)"
        );
        assert!(!fam.d.is_empty());
        assert_eq!(fam.z, "q");
        // determinism
        let again = generate_from_seed("p8kOmysnbc", PAGE_SEED, 19).unwrap();
        assert_eq!(fam, again);
    }
}
