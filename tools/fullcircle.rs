//! Full synthetic loop: CDP seed grab (live DOM) -> seedfamily generate ->
//! POST /login. The browser window must be open on p-bandai (CDP :9222).
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const CDP_PAGE_WS: &str = "ws://127.0.0.1:9222/devtools/page/EAEF0E84243BAA900706327264E0D461";

fn ws_connect(url: &str) -> TcpStream {
    let authority = url.trim_start_matches("ws://");
    let (hostport, path) = authority.split_once('/').unwrap();
    let mut s = TcpStream::connect(hostport).unwrap();
    let hs = format!(
        "GET /{} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
        path, hostport
    );
    s.write_all(hs.as_bytes()).unwrap();
    let mut buf = [0u8; 8192];
    let mut got = 0usize;
    loop {
        let n = s.read(&mut buf[got..]).unwrap();
        if n == 0 {
            break;
        }
        got += n;
        if String::from_utf8_lossy(&buf[..got]).contains("\r\n\r\n") {
            break;
        }
    }
    s
}

fn send(ws: &mut TcpStream, text: &str) {
    let p = text.as_bytes();
    let l = p.len();
    let mut frame = if l < 126 {
        vec![0x81, l as u8 | 0x80]
    } else {
        vec![0x81, 126 | 0x80, (l >> 8) as u8, (l & 0xff) as u8]
    };
    frame.extend_from_slice(&[0u8; 4]);
    frame.extend_from_slice(p);
    ws.write_all(&frame).unwrap();
}

fn recv(ws: &mut TcpStream) -> Option<String> {
    let mut head = [0u8; 2];
    ws.read_exact(&mut head).ok()?;
    let len = match head[1] & 0x7f {
        126 => {
            let mut e = [0u8; 2];
            ws.read_exact(&mut e).ok()?;
            u16::from_be_bytes(e) as usize
        }
        127 => {
            let mut e = [0u8; 8];
            ws.read_exact(&mut e).ok()?;
            u64::from_be_bytes(e) as usize
        }
        n => n as usize,
    };
    let mut p = vec![0u8; len];
    if len > 0 {
        ws.read_exact(&mut p).ok()?;
    }
    if head[0] & 0x0f == 1 {
        Some(String::from_utf8_lossy(&p).into_owned())
    } else {
        Some(String::new())
    }
}

fn evaluate(ws: &mut TcpStream, id: u32, expr: &str) -> Option<String> {
    let escaped = expr
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    send(
        ws,
        &format!(
            r#"{{"id":{id},"method":"Runtime.evaluate","params":{{"expression":"{escaped}","returnByValue":true}}}}"#
        ),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if let Some(message) = recv(ws)
            && message.contains(&format!("\"id\":{id}"))
        {
            let needle = format!("\"id\":{id}");
            let _ = needle;
            // value extract
            let key = "\"value\":\"";
            if let Some(start) = message.find(key) {
                let rest = &message[start + key.len()..];
                let end = rest.find('"').unwrap_or(rest.len());
                return Some(rest[..end].to_owned());
            }
            return Some(String::new());
        }
    }
    None
}

fn main() {
    let user =
        std::env::var("BANDAI_USER").unwrap_or_else(|_| "billymarquez@onezeromail.com".into());
    let pass = std::env::var("BANDAI_PASS").unwrap_or_else(|_| String::new());

    // 1. grab the LIVE seed + family from the page DOM
    let mut ws = ws_connect(CDP_PAGE_WS);
    let _ = evaluate(&mut ws, 1, "1");
    let seed = evaluate(
        &mut ws,
        2,
        "(document.querySelector('script[src*=seed]')||{src:''}).src.split('seed=')[1].split('&')[0]||''",
    )
    .unwrap_or_default();
    let family_raw = evaluate(
        &mut ws,
        3,
        "(document.querySelector('script[src*=seed]')||{src:''}).src.split('seed=')[1]?$'':''",
    )
    .unwrap_or_default();
    drop(ws);

    if seed.is_empty() {
        eprintln!("no live seed tag in DOM; is the bandai tab open?");
        std::process::exit(2);
    }
    // family from the src query (p8kOmysnbc--z=q)
    let family = if let Some(pos) = family_raw.find("p8k") {
        family_raw[pos..]
            .split('&')
            .next()
            .unwrap_or("p8kOmysnbc")
            .to_owned()
    } else {
        "p8kOmysnbc".to_owned()
    };
    println!("live seed: {} chars; family: {family}", seed.len());

    // 2. generate the family
    let built = match shape::seedfamily::generate_from_seed(&family, &seed, 0x00C0) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("generate: {e}");
            std::process::exit(3);
        }
    };

    // 3. login POST via reqwest with the generated family. The a-carrier is
    // the ENGINE-SEALED container over this session's signals (proven e2e
    // chain: seed_to_headers_roundtrip), never a replayed blob.
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36")
        .cookie_store(true)
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();
    // warm - FULL navigation chain per golden referer: /us (srsltid lands
    // here) -> login?redirect=... -> cart touch mints _BSP_CART_TOKEN_
    let _ = client
        .get("https://p-bandai.com/us?srsltid=test")
        .header("accept", "text/html")
        .send();
    let _ = client
        .get("https://p-bandai.com/us/login?redirectUrl=%2Fus%3Fsrsltid%3Dtest")
        .header("accept", "text/html")
        .header("referer", "https://p-bandai.com/us?srsltid=test")
        .send();
    let _ = client
        .get("https://p-bandai.com/us/cart")
        .header("accept", "text/html")
        .header(
            "referer",
            "https://p-bandai.com/us/login?redirectUrl=%2Fus%3Fsrsltid%3Dtest",
        )
        .send();
    let settings = client
        .get("https://p-bandai.com/us/api/frontend-settings")
        .header("accept", "application/json")
        .send()
        .and_then(|r| r.json::<serde_json::Value>())
        .ok();
    let csrf = settings
        .as_ref()
        .and_then(|v| v["csrfToken"].as_str())
        .unwrap_or_default()
        .to_owned();

    #[cfg(feature = "engine")]
    // engine-sealed a: pin the reference layout, encode this session's state
    let session_shape = shape::session::Session {
        transaction_token: "ab".into(),
        alphabet: shape::seedfamily::BANDAI_ALPHABET.to_string(),
        shuffle_key: vec![],
        keys: [0; 8],
    };
    #[cfg(feature = "engine")]
    let a_carrier =
        shape::engine_seal(&session_shape, &built, 0x00C0).unwrap_or_else(|_| built.c.clone());
    #[cfg(not(feature = "engine"))]
    let a_carrier = built.c.clone();
    // chunk split per fresh-200: head 5925 b64 chars, rest to a0
    let split_at = a_carrier.len().min(5925);
    let a_head = a_carrier[..split_at].to_owned();
    let a_tail = if a_carrier.len() > split_at {
        a_carrier[split_at..].to_owned()
    } else {
        String::new()
    };

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
        .header("priority", "u=1, i")
        .header("accept-language", "en")
        .header(
            "sec-ch-ua",
            "\"Not:A-Brand\";v=\"8\", \"Chromium\";v=\"156\", \"Google Chrome\";v=\"156\"",
        )
        .header("sec-ch-ua-mobile", "?0")
        .header("sec-ch-ua-platform", "\"Windows\"")
        .header("sec-fetch-dest", "empty")
        .header("sec-fetch-mode", "cors")
        .header("sec-fetch-site", "same-origin")
        .header(
            "referer",
            "https://p-bandai.com/us/login?redirectUrl=%2Fus%3Fsrsltid%3Dtest",
        )
        .header("origin", "https://p-bandai.com")
        // fresh-200 chunk naming: a carries the head, a0 the tail (split at
        // 5925 bytes observed; the a0 chunk only exists when content remains)
        .header(format!("{}-a", built.family), &a_head)
        .header(format!("{}-a0", built.family), &a_tail)
        .header(format!("{}-b", built.family), &built.b)
        .header(format!("{}-c", built.family), &built.c)
        .header(format!("{}-d", built.family), &built.d)
        .header(format!("{}-f", built.family), &built.f)
        .header(format!("{}-z", built.family), &built.z)
        .body(body)
        .send()
        .expect("login POST");

    let status = response.status().as_u16();
    println!("POST /login -> {status}");
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
        std::process::exit(4);
    }
}
