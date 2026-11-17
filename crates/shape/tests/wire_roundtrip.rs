// Ground-truth check with the ACTUAL crate code (not my trace copies):
// call shape::wire directly via a test in the crate.
use shape::wire::{read_integer, write_integer};

#[test]
fn trace() {
    for v in [4096i64, -123456, -32, 31, 0] {
        let mut buf = Vec::new();
        let mut none = || 0u8;
        write_integer(&mut buf, &mut none, v);
        let dec = read_integer(&buf);
        println!("v={v} enc={:02X?} dec={:?}", buf, dec);
        assert_eq!(dec.unwrap(), v, "roundtrip {v}");
    }
}
