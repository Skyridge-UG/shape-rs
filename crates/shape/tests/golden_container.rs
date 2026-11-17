//! Container-level probe: custom-alphabet decode of the golden a value and
//! structural assertions against the engine container grammar.

const GOLDEN_A: &str = include_str!("../../../tools/golden-a.txt");
const ALPHA: &str = include_str!("../../../tools/bandai-alphabet.txt");

/// Custom-b64 decode per the engine's symbol-table semantics.
fn decode(text: &str, alphabet: &str) -> Vec<u8> {
    let symbols: std::collections::HashMap<char, u8> =
        alphabet.chars().enumerate().map(|(i, c)| (c, i as u8)).collect();
    let mut out = Vec::new();
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for ch in text.chars() {
        let v = match symbols.get(&ch) {
            Some(v) => *v as u32,
            None => continue,
        };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
    }
    out
}

#[test]
fn golden_a_decodes_to_container_sized_blob() {
    let bytes = decode(GOLDEN_A, ALPHA);
    // 7754 symbols * 6 bits = 46524 bits = 5815.5 -> 5815 bytes
    assert_eq!(bytes.len(), 5815, "6-bit unpacked length");
    // Container head: engine superpack containers begin with a version nibble
    // followed by the section table; assert non-trivial entropy head.
    let zeros = bytes.iter().take(16).filter(|b| **b == 0).count();
    assert!(zeros < 12, "head should not be near-all-zero: {zeros}");
}

#[test]
fn alphabet_roundtrip_smoke() {
    let bytes = decode("pJJ", ALPHA);
    assert_eq!(bytes.len(), 2);
}