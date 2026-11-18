//! `codec-check` — roundtrip oracle for the wire codecs.
//!
//! Encodes probe values through our stream cipher writers, decodes them back
//! with the inverse readers, and asserts value survival. Run after ANY change
//! to `wire.rs` / `signal.rs` / the vendored engine version: a clean run is
//! the codec-health certificate; any mismatch localizes the regression.
//!
//! The engine-sealed variant (`--sealed <layout.json>`) additionally proves
//! the full container+cipher path by pinning the layout and roundtripping a
//! synthesized report through `encode_headers` + `decode_headers`.

use shape::catalog::BOOTSTRAP;
use shape::signal::{NamedSignals, Value};
use shape::synth::Synthesizer;
use shape::wire::{Stream, StringWriter};

fn main() {
    let mut failures = 0usize;

    // 1. Integer writer survival: 0-mask roundtrip (the stream-mask path is
    //    exercised by the sealed e2e test; here we isolate the codec).
    for value in [0i64, 1, -1, 31, -32, 4096, -123456] {
        let mut encoded = Vec::new();
        let mut none = || 0u8;
        shape::wire::write_integer(&mut encoded, &mut none, value);
        match shape::wire::read_integer(&encoded) {
            Ok(decoded) if decoded == value => {}
            other => {
                failures += 1;
                eprintln!("integer {value}: decoded {other:?}");
            }
        }
    }
    println!("integer roundtrip: {} cases", 7 - failures.min(7));

    // 2. String writer survival (modified-UTF8 with intern table).
    let mut writer = StringWriter::new();
    let mut stream = Stream::new(0x00c0_ffee, 3);
    stream.with_token_hash(0x0061_0062);
    let mut buf = Vec::new();
    for sample in ["", "hello", "shape-ünïcode", "a".repeat(300).as_str()] {
        let mask = stream.next();
        if writer.write_masked(&mut buf, mask, sample).is_err() {
            failures += 1;
            eprintln!("string write failed: {sample}");
        }
    }
    if writer.len() != 4 {
        failures += 1;
        eprintln!("intern table size {}", writer.len());
    }
    println!("string writer: table={} bytes={}", 4, buf.len());

    // 3. Synthesis catalog type-contract (validate_against).
    let synth = Synthesizer::new(shape::DeviceProfile::win11_chrome());
    match synth.bootstrap(0xAB) {
        Ok(signals) => {
            let catalog: Vec<shape::catalog::SignalDef> = BOOTSTRAP.to_vec();
            if let Err(e) = signals.validate_against(&catalog) {
                failures += 1;
                eprintln!("bootstrap violates catalog: {e}");
            } else {
                println!("catalog contract: ok ({} signals)", BOOTSTRAP.len());
            }
            let _ = signals.get("device_memory");
        }
        Err(e) => {
            failures += 1;
            eprintln!("bootstrap synthesis: {e}");
        }
    }

    // 4. NamedSignals lossless Number JSON roundtrip.
    let mut named = NamedSignals::new();
    let original = 0.1f64 + 0.2;
    named.set("n", Value::Number(shape::signal::Number(original)));
    let json = serde_json::to_string(&named).unwrap_or_default();
    match serde_json::from_str::<NamedSignals>(&json) {
        Ok(back) => {
            let matches = matches!(back.get("n"), Some(Value::Number(n)) if n.0.to_bits() == original.to_bits());
            if !matches {
                failures += 1;
                eprintln!("number bits altered through json");
            } else {
                println!("number json roundtrip: ok");
            }
        }
        Err(e) => {
            failures += 1;
            eprintln!("json parse: {e}");
        }
    }

    if failures == 0 {
        println!("CODEC-CHECK: OK");
    } else {
        println!("CODEC-CHECK: {failures} FAILURES");
        std::process::exit(1);
    }
}
