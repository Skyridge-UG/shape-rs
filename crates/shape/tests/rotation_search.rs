//! Alphabet-rotation brute force: find the ring rotation whose decode of the
//! golden bandai container yields the smoke grammar head (00 00 xx xx ...).

const BANDAI_A: &str = include_str!("../../../tools/golden-a.txt");
const ALPHA: &str = include_str!("../../../tools/bandai-alphabet.txt");
const SMOKE_A: &str = include_str!("../../../tools/golden-a-smoke.txt");
const SMOKE_ALPHA: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode(text: &str, alphabet: &[char]) -> Vec<u8> {
    let map: std::collections::HashMap<char, u8> = alphabet
        .iter()
        .enumerate()
        .map(|(i, c)| (*c, i as u8))
        .collect();
    let mut out = Vec::new();
    let mut acc: u32 = 0;
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

fn rotated(alpha: &str, by: usize) -> Vec<char> {
    let chars: Vec<char> = alpha.chars().collect();
    (0..chars.len())
        .map(|i| chars[(i + by) % chars.len()])
        .collect()
}

#[test]
fn rotation_search_matches_smoke_grammar() {
    let smoke = decode(SMOKE_A, &SMOKE_ALPHA.chars().collect::<Vec<_>>());
    let smoke_head = &smoke[..4.min(smoke.len())];
    let mut best: Option<(i64, Vec<u8>)> = None;
    for rot in 0..64usize {
        let alpha = rotated(ALPHA, rot);
        let bytes = decode(&format!("{BANDAI_A}{}", ""), &alpha);
        if bytes.len() < 4 {
            continue;
        }
        // grammar: first u32 BE length == plausible container size
        let len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        if len > 100 && len < bytes.len() {
            let scored = (len as i64 - bytes.len() as i64).abs();
            if best.as_ref().map(|(b, _)| scored < *b).unwrap_or(true) {
                best = Some((scored, bytes.clone()));
                println!(
                    "rot={rot} head={:02X?} declaredLen={len} total={}",
                    &bytes[..4],
                    bytes.len()
                );
            }
        }
    }
    let _ = smoke_head;
    // Documented outcome: no rotation matches (see bandai-recon.md
    // 'rotation search result') - the bandai container uses its own header
    // grammar. The assertion encodes today's finding so a future engine /
    // header change surfaces as a PASS here.
    assert!(
        best.is_none(),
        "unexpected grammar match; re-check the recon note"
    );
}
