//! Dump the smoke sealed a-carrier for the slot diff.
#![cfg(feature = "engine")]
use shape::pin::PinInput;
use shape::session::Session;

#[test]
fn dump_smoke_a() {
    let session = Session {
        transaction_token: "ab".into(),
        alphabet: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/".into(),
        shuffle_key: vec![],
        keys: [0; 8],
    };
    let program = shape::pin::compose_reference_program_pub();
    let pinned = shape::pin::pin(&PinInput {
        source: &program,
        session: &session,
    })
    .expect("pin");
    let mut signals = shape::signal::NamedSignals::new();
    signals
        .set(
            "storage_access_api_presence",
            shape::signal::Value::Bool(true),
        )
        .set("device_memory", shape::signal::Value::Number(9.0.into()));
    let bundle = shape::encode::encode_headers(&pinned.engine, &signals, [19, 23]).expect("enc");
    for (i, chunk) in bundle.a.iter().enumerate() {
        println!("SMOKECHUNK{i}:{chunk}");
    }
}
