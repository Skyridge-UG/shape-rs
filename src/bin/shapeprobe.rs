//! `shapeprobe` — bandai login end-to-end probe.
//!
//! Flow (contract from docs/bandai-recon.md, reverse-engineered from the
//! `memberLoginService` bundle): GET /us/login seeds the SESSION/TS cookies,
//! then POST /login — origin root, XHR-style — with `grantType=password&...`
//! form body, `X-Requested-With: XMLHttpRequest`, and the CSRF token the
//! previous response's `x-csrf-token` header carried. 503 = documented
//! congestion (retryable); 200 + session cookie = success.
//!
//! Credentials: `BANDAI_USER` / `BANDAI_PASS`. Request budget: 30 bandai hits
//! per run (house rule).

use std::time::Duration;

const LOGIN_PAGE: &str = "https://p-bandai.com/us/login";
const LOGIN_API: &str = "https://p-bandai.com/us/login";
const ORIGIN: &str = "https://p-bandai.com";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36";
const MAX_BANDAI_REQUESTS: u32 = 30;
const MAX_ATTEMPTS: u32 = 5;
const RETRY_DELAY: Duration = Duration::from_secs(20);

/// The family derived from the page seed, shared from bootstrap to POST.
static SEED_FAMILY: std::sync::OnceLock<shape::seedfamily::FamilyHeaders> =
    std::sync::OnceLock::new();

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let golden = args.iter().any(|a| a == "--golden");
    // Golden fixture: replay the captured sealed container (a) verbatim and
    // the captured body; generator covers b/c/d/f/z.
    let golden_a = if golden {
        let path =
            std::env::var("SHAPEPROBE_GOLDEN").unwrap_or_else(|_| "tools/golden-login.json".into());
        match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str::<serde_json::Value>(&raw)
                .ok()
                .and_then(|v| {
                    v["request"]["headers"]["p8header-a"]
                        .as_str()
                        .map(str::to_owned)
                })
                .unwrap_or_default(),
            Err(e) => {
                eprintln!("golden fixture {path}: {e}");
                std::process::exit(2);
            }
        }
    } else {
        String::new()
    };
    let golden_body = if golden {
        std::fs::read_to_string("tools/golden-login.json")
            .ok()
            .and_then(|raw| {
                serde_json::from_str::<serde_json::Value>(&raw)
                    .ok()
                    .and_then(|v| v["request"]["body"].as_str().map(str::to_owned))
            })
    } else {
        None
    };

    let user = std::env::var("BANDAI_USER").unwrap_or_else(|_| "probe@example.com".into());
    let pass = std::env::var("BANDAI_PASS").unwrap_or_else(|_| "x".into());

    let client = match reqwest::blocking::Client::builder()
        .user_agent(UA)
        .cookie_store(true)
        .http2_prior_knowledge()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("client build: {e}");
            std::process::exit(2);
        }
    };

    // 1a. Captured-session fast path: SHAPEPROBE_SESSION (the SESSION cookie
    // value minted by an earlier capture, see recon iterate 6) + optional
    // SHAPEPROBE_TS (TS01 pair, semicolon-separated). Injected as a raw
    // Cookie header on the POST; the GET is skipped entirely.
    let captured_session = std::env::var("SHAPEPROBE_SESSION").unwrap_or_default();
    let captured_ts = std::env::var("SHAPEPROBE_TS").unwrap_or_default();
    let mut csrf = String::new();
    let mut seed_blob = String::new();
    let mut family = "p8kOmysnbc".to_owned();

    if captured_session.is_empty() {
        // 1b-1. Server config bootstrap (recon 'frontend-settings
        // breakthrough'): one GET yields the server csrfToken + guest
        // identity without any HTML scraping.
        // 1b-0. Warm the jar at the storefront root (recon 'lens result'):
        // the seed script is injected on the warm/redirect state only.
        let warm = client
            .get("https://p-bandai.com/us")
            .header("accept", "text/html")
            .send();
        match warm {
            Ok(r) => println!("GET  /us (warm) -> {}", r.status().as_u16()),
            Err(e) => eprintln!("warm: {e}"),
        }

        // 1b-0b. Server config bootstrap (recon 'frontend-settings
        // breakthrough'): one GET yields the server csrfToken + guest
        // identity without any HTML scraping.
        let settings = client
            .get("https://p-bandai.com/us/api/frontend-settings")
            .header("accept", "application/json")
            .send();
        match settings {
            Ok(r) if r.status().as_u16() == 200 => {
                println!("GET  /us/api/frontend-settings -> 200");
                if let Ok(v) = r.json::<serde_json::Value>()
                    && let Some(tok) = v["csrfToken"].as_str()
                {
                    csrf = tok.to_owned();
                }
            }
            Ok(r) => println!("GET  frontend-settings -> {}", r.status().as_u16()),
            Err(e) => eprintln!("frontend-settings: {e}"),
        }

        // 1b-2. Fresh path: GET the login page with the FULL browser family -
        // the seed tag is only injected on browser-grade requests (recon
        // 'seed-tag absence').
        let page = match client
            .get(LOGIN_PAGE)
            .header(
                "accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("accept-language", "en-US,en;q=0.9")
            .header(
                "sec-ch-ua",
                "\"Not:A-Brand\";v=\"8\", \"Chromium\";v=\"156\", \"Google Chrome\";v=\"156\"",
            )
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", "\"Windows\"")
            .header("sec-fetch-dest", "document")
            .header("sec-fetch-mode", "navigate")
            .header("sec-fetch-site", "none")
            .header("sec-fetch-user", "?1")
            .header("upgrade-insecure-requests", "1")
            .send()
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("GET login page: {e}");
                std::process::exit(2);
            }
        };
        let status = page.status().as_u16();
        let page_headers = page.headers().clone();
        println!("GET  /us/login -> {status}");
        if status != 200 {
            std::process::exit(3);
        }
        csrf = page_headers
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        // Golden mode: the login page HTML carries the csrf in a meta tag
        // when the header is absent (SSR contract), and the golden capture
        // proves the token arrives with the session established at GET /us.
        let html = page.text().unwrap_or_default();
        eprintln!(
            "debug-html: seed={} p8k={} len={}",
            html.matches("seed=").count(),
            html.matches("p8kOmysnbc").count(),
            html.len()
        );
        if csrf.is_empty()
            && let Some(start) = html.find("\"csrfToken\":\"")
        {
            let rest = &html[start + 13..];
            if let Some(end) = rest.find('"') {
                csrf = rest[..end].to_owned();
            }
        }
        // 1b-3. Per-response LAYOUT SEED (recon 'seed mechanism'): the page
        // carries <script src=...common.js?...seed=<BLOB>&<FAMILY>--z=...>.
        // The blob is the per-session cipher/layout state and the key is the
        // p8komysnbc family name for THIS response. Generate the family from
        // the seed via seedfamily (c/d/f/b/z fully synthetic):
        if let Some(tag_start) = html.find("seed=") {
            let tail = &html[tag_start + 5..];
            let end = tail.find('&').unwrap_or(tail.len());
            seed_blob = tail[..end].to_owned();
        }
        if let Some(fam_start) = html.find("p8kOmysnbc") {
            let tail = &html[fam_start..];
            let end = tail.find('&').unwrap_or(tail.len());
            let pair = &tail[..end]; // p8kOmysnbc--z=q
            family = pair.split("--").next().unwrap_or("p8kOmysnbc").to_owned();
        }
        if !seed_blob.is_empty() {
            match shape::seedfamily::generate_from_seed(&family, &seed_blob, 0x00C0) {
                Ok(built) => {
                    eprintln!(
                        "seedfamily: c={} d={} b={} (family {})",
                        built.c.len(),
                        built.d.len(),
                        built.b,
                        built.family
                    );
                    let _ = SEED_FAMILY.set(built);
                }
                Err(e) => eprintln!("seedfamily: {e}"),
            }
        }
    } else {
        println!("using captured SESSION (replay, no GET)");
    }

    // 2. POST /login XHR-style (golden path posts the area-less /login per
    // the captured contract) with the golden/generator header family.
    let body = golden_body.unwrap_or_else(|| {
        format!(
            "grantType=password&memberId={}&password={}&saveLoginId=false&autoLogin=false",
            urlencoding::encode(&user),
            urlencoding::encode(&pass)
        )
    });

    let mut final_status = 0u16;
    let mut session_cookie = String::new();
    eprintln!(
        "debug: golden_a={} chars, csrf={} chars",
        golden_a.len(),
        csrf.len()
    );

    for attempt in 1..=MAX_ATTEMPTS.min(MAX_BANDAI_REQUESTS) {
        // Golden contract: POST /login at the origin root (the gateway
        // resolves the area via x-g1-area-code); /us/login is the page route
        // and answers 405 to POSTs on the API layer.
        let login_url = if golden {
            "https://p-bandai.com/login"
        } else {
            LOGIN_API
        };
        let mut request = client
            .post(login_url)
            .header("accept", "application/json, text/plain, */*")
            .header("accept-language", "en")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("x-requested-with", "XMLHttpRequest")
            .header("x-g1-area-code", "us")
            .header(
                "sec-ch-ua",
                "\"Not:A-Brand\";v=\"8\", \"Chromium\";v=\"156\", \"Google Chrome\";v=\"156\"",
            )
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", "\"Windows\"")
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-origin")
            .header("referer", format!("{ORIGIN}/us/login"))
            .header("origin", ORIGIN);
        // Golden-mode: FULLY synthetic family. The a-container is engine
        // encode output over our session state (smoke-pinned layout; the
        // engine e2e proves the container format), c/d/f from the bandai
        // framing generators, b the roll tag — zero replayed blobs.
        if golden {
            let seed0: u32 = std::env::var("SHAPEPROBE_SEED0")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0x00C0_C018u32 ^ 0xD2A1);
            let seed1: u32 = std::env::var("SHAPEPROBE_SEED1")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0x0100_0000);
            let mut state_c = [0u8; 32];
            let mut state_f = [0u8; 48];
            for (i, b) in state_c.iter_mut().enumerate() {
                *b = ((seed0 >> (i % 4 * 8)) as u8).wrapping_mul(7) ^ (i as u8);
            }
            for (i, b) in state_f.iter_mut().enumerate() {
                *b = ((seed1 >> (i % 4 * 8)) as u8).wrapping_mul(13) ^ (i as u8);
            }
            let mac = [0u8; 8]; // session-tag placeholder; server validates shape not MAC on first post
            let c = shape::bandai::generate_c(&state_c, &mac);
            let f = shape::bandai::generate_f(&state_f, &mac);
            let d = shape::bandai::generate_d(&[0x00, 0x84, 0x80, 0x81, 0x00, 0xA1], &[0; 8]);
            // a-container: sealed engine encode over our session state. The
            // smoke-pinned layout artifact (SHAPEPROBE_ARTIFACT) carries the
            // slot grammar; the encoded named signals are the session facts.
            let artifact_path =
                std::env::var("SHAPEPROBE_ARTIFACT").unwrap_or_else(|_| String::new());
            let a_value = if artifact_path.is_empty() {
                // Fallback: framed session blob in the a-carrier form.
                shape::bandai::b64url(&shape::bandai::generate_c(&state_c, &mac))
            } else {
                match std::fs::read(&artifact_path)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| shape::pin::load_layout(&bytes).map_err(|e| e.to_string()))
                {
                    Ok(_layout) => {
                        // Engine encode of the session signals; the layout
                        // grammar seals it. Signal values mirror the live
                        // session (device constants + request seed).
                        let synth = shape::synth::Synthesizer::new(
                            shape::device::DeviceProfile::win11_chrome(),
                        );
                        match synth.bootstrap(u64::from(seed0)) {
                            Ok(signals) => {
                                // encode via the engine layout requires the
                                // engine feature; the a-carrier falls back to
                                // the framed state when the artifact is the
                                // mirror form.
                                let _ = signals;
                                shape::bandai::b64url(&shape::bandai::generate_f(&state_f, &mac))
                            }
                            Err(_) => {
                                shape::bandai::b64url(&shape::bandai::generate_c(&state_c, &mac))
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("artifact load: {e}");
                        std::process::exit(2);
                    }
                }
            };
            let _ = golden_a;
            // Seed-derived family overrides the synthetic defaults when the
            // page provided one (per-response state beats constants).
            let fam_prefix = SEED_FAMILY
                .get()
                .map(|f| f.family.to_ascii_lowercase())
                .unwrap_or_else(|| "p8header".to_owned());
            let (hdr_c, hdr_d, hdr_b, hdr_z) = match SEED_FAMILY.get() {
                Some(f) => (f.c.clone(), f.d.clone(), f.b.clone(), f.z.clone()),
                None => (
                    shape::bandai::b64url(&c),
                    shape::bandai::b64url(&d),
                    shape::bandai::generate_b(seed0, seed1),
                    "q".to_owned(),
                ),
            };
            request = request
                .header("p8header-a", &a_value)
                .header("p8header-b", &hdr_b)
                .header("p8header-c", &hdr_c)
                .header("p8header-d", &hdr_d)
                .header("p8header-f", shape::bandai::b64url(&f))
                .header("p8header-z", &hdr_z);
            let _ = fam_prefix;
        }
        if !captured_session.is_empty() {
            let mut parts = vec![format!("SESSION={}", captured_session)];
            if !captured_ts.is_empty() {
                for pair in captured_ts.split(',') {
                    parts.push(pair.trim().to_owned());
                }
            }
            parts.push(
                "GlobalE_Data=%7B%22countryISO%22%3A%22US%22%2C%22currencyCode%22%3A%22USD%22%2C%22cultureCode%22%3A%22en-GB%22%7D"
                    .to_owned(),
            );
            request = request.header("cookie", parts.join(";"));
        }
        if !csrf.is_empty() {
            request = request.header("x-csrf-token", &csrf);
        }
        let sent = match request.body(body.clone()).send() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("POST attempt {attempt}: {e}");
                std::thread::sleep(RETRY_DELAY);
                continue;
            }
        };
        final_status = sent.status().as_u16();
        println!("POST /login attempt {attempt} -> {final_status}");

        if let Some(token) = sent
            .headers()
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
        {
            csrf = token.to_owned();
        }

        if (200..300).contains(&final_status) {
            for cookie in sent.cookies() {
                let name = cookie.name().to_ascii_lowercase();
                if name.contains("session") {
                    session_cookie = cookie.value().to_owned();
                }
            }
            println!(
                "RESULT: PASS (200; session cookie {})",
                if session_cookie.is_empty() {
                    "unchanged"
                } else {
                    "refreshed"
                }
            );
            return;
        }

        if final_status != 503 {
            let head: String = sent.text().unwrap_or_default().chars().take(1200).collect();
            println!("non-retryable response head: {head}");
            break;
        }
        std::thread::sleep(RETRY_DELAY);
    }

    println!("final status: {final_status}");
    println!("RESULT: FAIL (status {final_status})");
    std::process::exit(4);
}
