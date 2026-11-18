//! fullcircle-wreq: identical loop, wreq transport (Chrome TLS emulation).
//! The A/B against the reqwest build isolates fingerprint vs content as the
//! 501 cause - Justin's point: the edge sees the TLS handshake first.

use std::time::Duration;

fn main() {
    let user =
        std::env::var("BANDAI_USER").unwrap_or_else(|_| "billymarquez@onezeromail.com".into());
    let pass = std::env::var("BANDAI_PASS").unwrap_or_else(|_| String::new());

    // Chrome-emulated TLS/HTTP2 stack, async runtime in place. With
    // BANDAI_COOKIES set (cookie-grab output) the jar is pre-seeded with the
    // live tab's session identity - the SESSION/TS01 pair the seed was
    // minted against.
    let mut builder = wreq::Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(20))
        .emulation(wreq_util::emulate::Profile::Chrome137);
    let mut seeded = 0usize;
    if let Ok(cookie_file) = std::env::var("BANDAI_COOKIES")
        && let Ok(text) = std::fs::read_to_string(&cookie_file)
    {
        let jar = wreq::cookie::Jar::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            jar.add(
                format!("{line}; Domain=p-bandai.com; Path=/; Secure"),
                "https://p-bandai.com/",
            );
            seeded += 1;
        }
        builder = builder.cookie_provider(std::sync::Arc::new(jar));
    }
    let rt = tokio::runtime::Runtime::new().expect("tokio rt");
    let result = rt.block_on(async move {
        let client = builder.build().expect("wreq client");
        println!("jar seeded: {seeded} cookies");
        let _ = client
            .get("https://p-bandai.com/us")
            .header("accept", "text/html")
            .send()
            .await;
        let _ = client
            .get("https://p-bandai.com/us/login")
            .header("accept", "text/html")
            .send()
            .await;
        let csrf = match client
            .get("https://p-bandai.com/us/api/frontend-settings")
            .header("accept", "application/json")
            .send()
            .await
        {
            Ok(r) => r
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v: serde_json::Value| v["csrfToken"].as_str().map(str::to_owned))
                .unwrap_or_default(),
            Err(_) => String::new(),
        };
        println!("csrf: {} chars", csrf.len());

        let seed = std::fs::read_to_string("tools/bandai-seed.txt").unwrap_or_default();
        let family = "p8kOmysnbc";
        let built =
            shape::seedfamily::generate_from_seed(family, seed.trim(), 0x00C0).expect("gen");

        let body = format!(
            "grantType=password&memberId={}&password={}&saveLoginId=false&autoLogin=false",
            urlencoding::encode(&user),
            urlencoding::encode(&pass)
        );
        let response = client
            .post("https://p-bandai.com/login")
            .header("accept", "application/json, text/plain, */*")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("x-requested-with", "XMLHttpRequest")
            .header("x-g1-area-code", "us")
            .header("x-csrf-token", &csrf)
            .header("origin", "https://p-bandai.com")
            .header("referer", "https://p-bandai.com/us/login")
            .header(format!("{}-a", built.family), &built.a)
            .header(format!("{}-b", built.family), &built.b)
            .header(format!("{}-c", built.family), &built.c)
            .header(format!("{}-d", built.family), &built.d)
            .header(format!("{}-f", built.family), &built.f)
            .header(format!("{}-z", built.family), &built.z)
            .body(body)
            .send()
            .await
            .expect("login POST");
        let status = response.status().as_u16();
        println!("POST /login (wreq chrome137) -> {status}");
        let mut session = String::new();
        for cookie in response.cookies() {
            if cookie.name().eq_ignore_ascii_case("SESSION") {
                session = cookie.value().to_owned();
            }
        }
        if status == 200 {
            println!("SESSION: {}", &session[..session.len().min(24)]);
            println!("RESULT: PASS");
        } else {
            println!("RESULT: FAIL ({status})");
        }
        status
    });

    if result != 200 {
        std::process::exit(4);
    }
}
