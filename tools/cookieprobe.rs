// raw-cookie probe: explicit Cookie header from file, no jar, wreq stack.
use std::time::Duration;

fn main() {
    let cookies = std::fs::read_to_string("D:/tab-cookies.txt").expect("cookies");
    let cookie_header = cookies
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let client = wreq::Client::builder()
            .timeout(Duration::from_secs(20))
            .emulation(wreq_util::emulate::Profile::Chrome137)
            .build()
            .unwrap();
        // 1. settings WITH explicit cookie header: session recognized?
        let r = client
            .get("https://p-bandai.com/us/api/frontend-settings")
            .header("accept", "application/json")
            .header("cookie", &cookie_header)
            .send()
            .await
            .unwrap();
        let ct = r
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let body = r.text().await.unwrap_or_default();
        println!("settings+cookie: ct={ct} len={} head={}", body.len(), {
            let b: String = body.chars().take(60).collect();
            b
        });
        // 2. login POST with explicit cookie header + fresh-seed family
        let seed = std::fs::read_to_string("tools/bandai-seed.txt").unwrap_or_default();
        let built = shape::seedfamily::generate_from_seed("p8kOmysnbc", seed.trim(), 0x00C0)
            .expect("gen");
        let body = "grantType=password&memberId=billymarquez%40onezeromail.com&password=x&saveLoginId=false&autoLogin=false";
        let r = client
            .post("https://p-bandai.com/login")
            .header("accept", "application/json, text/plain, */*")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("x-requested-with", "XMLHttpRequest")
            .header("cookie", &cookie_header)
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
            .unwrap();
        println!("POST w/ cookie header -> {}", r.status().as_u16());
    });
}
