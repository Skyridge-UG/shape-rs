//! Slot-structure diff: decode BOTH the smoke golden container and the
//! bandai golden container with the same custom-alphabet reader, and print
//! their leading structure for the layout diff.

const SMOKE_A: &str = include_str!("../../../tools/golden-a-smoke.txt");
const BANDAI_A: &str = include_str!("../../../tools/golden-a.txt");
const SMOKE_ALPHA: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const BANDAI_ALPHA: &str = include_str!("../../../tools/bandai-alphabet.txt");

fn decode(text: &str, alphabet: &str) -> Vec<u8> {
    let map: std::collections::HashMap<char, u8> =
        alphabet.chars().enumerate().map(|(i, c)| (c, i as u8)).collect();
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

fn head(bytes: &[u8], n: usize) -> String {
    bytes.iter().take(n).map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

#[test]
fn dump_both_container_heads() {
    let smoke = decode(SMOKE_A, SMOKE_ALPHA);
    let bandai = decode(BANDAI_A, BANDAI_ALPHA);
    println!("smoke  len={} head: {}", smoke.len(), head(&smoke, 24));
    println!("bandai len={} head: {}", bandai.len(), head(&bandai, 24));
    // Both are engine superpack containers: share the version nibble family.
    assert!(!smoke.is_empty());
    assert!(!bandai.is_empty());
}
