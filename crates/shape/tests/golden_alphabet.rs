//! Golden-a alphabet validation + container decode probe.

const GOLDEN_A: &str = include_str!("../../../tools/golden-a.txt");
const ALPHA: &str = include_str!("../../../tools/bandai-alphabet.txt");

#[test]
fn golden_a_uses_bandai_alphabet_exactly() {
    for ch in GOLDEN_A.chars() {
        assert!(
            ALPHA.contains(ch),
            "char {ch} outside the extracted bandai alphabet"
        );
    }
}

#[test]
fn alphabet_has_64_unique_symbols() {
    let mut seen = [false; 128];
    let mut count = 0;
    for ch in ALPHA.chars() {
        let b = ch as usize;
        assert!(!seen[b], "dup {ch}");
        seen[b] = true;
        count += 1;
    }
    assert_eq!(count, 64);
}