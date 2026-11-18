//! fullcircle-proxy: the Rust orchestrator drives the login THROUGH the
//! live page's network stack (cdp Runtime.evaluate + fetch), reusing the
//! browser's warm h2 session. The solver generates everything; the browser
//! socket carries it. Closes the 501-vs-503 connection delta.

use std::io::{Read, Write};
use std::net::TcpStream;

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
            r#"{{"id":{id},"method":"Runtime.evaluate","params":{{"expression":"{escaped}","awaitPromise":true,"returnByValue":true}}}}"#
        ),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if let Some(message) = recv(ws)
            && message.contains(&format!("\"id\":{id}"))
        {
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
    let ws_url = std::env::args().nth(1).unwrap_or_else(|| {
        "ws://127.0.0.1:9222/devtools/page/EAEF0E84243BAA900706327264E0D461".into()
    });

    // 1. solver generates the family from the live page seed (CDP read)
    let mut ws = ws_connect(&ws_url);
    let seed = evaluate(
        &mut ws,
        1,
        "(document.querySelector('script[src*=seed]')||{src:location.href}).src.split('seed=')[1].split('&')[0]",
    )
    .unwrap_or_default();
    if seed.is_empty() {
        eprintln!("no live seed; reload the bandai tab first");
        std::process::exit(2);
    }
    println!("live seed: {} chars", seed.len());
    let built = shape::seedfamily::generate_from_seed("p8komysnbc", &seed, 0x00C0)
        .expect("family generate");

    // 2. drive the login THROUGH the page (warm h2, browser jar, edge state)
    let js = format!(
        r#"(async () => {{
            const resp = await fetch('/login', {{
                method: 'POST',
                headers: {{
                    'accept': 'application/json, text/plain, */*',
                    'content-type': 'application/x-www-form-urlencoded',
                    'x-requested-with': 'XMLHttpRequest',
                    'x-g1-area-code': 'us',
                    'p8komysnbc-a': '{a}',
                    'p8komysnbc-b': '{b}',
                    'p8komysnbc-c': '{c}',
                    'p8komysnbc-d': '{d}',
                    'p8komysnbc-f': '{f}',
                    'p8komysnbc-z': '{z}',
                }},
                body: 'grantType=password&memberId={user}&password={pass}&saveLoginId=false&autoLogin=false',
                credentials: 'include',
            }});
            const sess = (resp.headers.get('set-cookie')||'');
            return 'status='+resp.status;
        }})()"#,
        a = built.a,
        b = built.b,
        c = built.c,
        d = built.d,
        f = built.f,
        z = built.z,
        user = urlencoding::encode(&user),
        pass = urlencoding::encode(&pass),
    );
    let result = evaluate(&mut ws, 2, &js).unwrap_or_default();
    println!("proxied login: {result}");

    if result.contains("status=200") {
        println!("RESULT: PASS");
    } else {
        println!("RESULT: FAIL");
        std::process::exit(4);
    }
}
