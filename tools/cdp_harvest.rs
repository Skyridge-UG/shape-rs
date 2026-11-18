//! CDP driver: navigate to bandai login, harvest the p8komysnbc minter
//! chunk via the debugger, and capture a real login's network traffic.
//!
//! Uses raw WebSocket CDP (no chromeoxide dep churn): Page.navigate +
//! Runtime.evaluate to instrument fetch/XHR, then Network.enable to record
//! every response body; the minter is the JS that constructs the first
//! p8komysnbc value.

use std::io::{Read, Write};
use std::net::TcpStream;

const CDP_HTTP: &str = "127.0.0.1:9222";

fn http_get(path: &str) -> String {
    let mut stream = TcpStream::connect(CDP_HTTP).expect("cdp tcp");
    let request = format!("GET {path} HTTP/1.1\r\nHost: {CDP_HTTP}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    let mut body = String::new();
    let _ = stream.read_to_string(&mut body);
    body
}

/// Minimal WebSocket client for one CDP session (no TLS, handshake + frames).
struct Ws {
    stream: TcpStream,
}

impl Ws {
    fn set_nonblocking(&mut self, on: bool) {
        let _ = self.stream.set_nonblocking(on);
    }
    fn connect(url: &str) -> Self {
        // url: ws://127.0.0.1:9222/devtools/page/XXXX
        let authority = url.trim_start_matches("ws://");
        let (hostport, path) = match authority.split_once('/') {
            Some((h, p)) => (h, format!("/{}", p)),
            None => (authority, "/".to_owned()),
        };
        let mut stream = TcpStream::connect(hostport).expect("ws tcp");
        let key = "dGhlIHNhbXBsZSBub25jZQ=="; // RFC sample; Chrome accepts any
        let handshake = format!(
            "GET {path} HTTP/1.1\r\nHost: {hostport}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        stream.write_all(handshake.as_bytes()).unwrap();
        // read until \r\n\r\x to consume the 101 response headers
        let mut buf = [0u8; 4096];
        let mut got = 0usize;
        loop {
            let n = stream.read(&mut buf[got..]).unwrap();
            if n == 0 {
                break;
            }
            got += n;
            if String::from_utf8_lossy(&buf[..got]).contains("\r\n\r\n") {
                break;
            }
        }
        Self { stream }
    }

    fn send_text(&mut self, text: &str) {
        let payload = text.as_bytes();
        let len = payload.len();
        let header = if len < 126 {
            vec![0x81, len as u8 | 0x80]
        } else {
            vec![0x81, 126 | 0x80, (len >> 8) as u8, (len & 0xff) as u8]
        };
        // client frames are masked; mask key zeros is legal
        let mask = [0u8; 4];
        let mut frame = header;
        frame.extend_from_slice(&mask);
        frame.extend_from_slice(payload);
        self.stream.write_all(&frame).unwrap();
    }

    fn recv_text(&mut self) -> Option<String> {
        let mut head = [0u8; 2];
        self.stream.read_exact(&mut head).ok()?;
        let opcode = head[0] & 0x0f;
        let len = match head[1] & 0x7f {
            126 => {
                let mut e = [0u8; 2];
                self.stream.read_exact(&mut e).ok()?;
                u16::from_be_bytes(e) as usize
            }
            127 => {
                let mut e = [0u8; 8];
                self.stream.read_exact(&mut e).ok()?;
                u64::from_be_bytes(e) as usize
            }
            n => n as usize,
        };
        let mut payload = vec![0u8; len];
        if len > 0 {
            self.stream.read_exact(&mut payload).ok()?;
        }
        if opcode == 1 {
            Some(String::from_utf8_lossy(&payload).into_owned())
        } else {
            Some(String::new()) // ping/pong/close: skip
        }
    }
}

/// The `.js` url of a Network.responseReceived frame, if any.
fn js_url_of(message: &str) -> Option<String> {
    let url = field(message, "url")?;
    if url.contains(".js") { Some(url) } else { None }
}

fn main() {
    eprintln!("harvester: start");
    // 1. take the bandai tab ws url from argv (created via PUT /json/new).
    let ws_line = std::env::args()
        .nth(1)
        .expect("usage: cdp-harvest <ws://.../devtools/page/ID>");
    eprintln!("harvester: ws {ws_line}");

    println!("tab ws: {ws_line}");
    eprintln!("harvester: connecting ws");
    let mut ws = Ws::connect(&ws_line);
    eprintln!("harvester: ws up");

    // 2. enable network + navigate reload with instrumentation
    for (id, method) in [
        ("1", "Network.enable"),
        ("2", "Page.enable"),
        ("3", "Runtime.enable"),
    ] {
        ws.send_text(&format!(r#"{{"id":{id},"method":"{method}"}}"#));
    }
    ws.send_text(r#"{"id":4,"method":"Page.reload"}"#);
    eprintln!("harvester: reload issued, draining 25s");

    // 3. drain events, collecting JS urls until quiet. The naive recv never
    // sees responseReceived because Page.reload floods events faster than a
    // single blocking read consumes them interleaved with pings; instead
    // poll with short reads and count ALL frames.
    let mut seen_js: Vec<String> = Vec::new();
    let mut frames = 0usize;
    let mut dumped = 0usize;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
    ws.stream.set_nonblocking(true).ok();

    while std::time::Instant::now() < deadline {
        match ws.recv_text() {
            Some(message) if !message.is_empty() => {
                if std::env::var("CDP_DUMP").is_ok() && dumped < 40 {
                    dumped += 1;
                    std::fs::write(format!("D:\\cdp-f{:03}.json", dumped), &message).ok();
                }
                frames += 1;
                if message.contains("Network.responseReceived")
                    && let Some(url) = js_url_of(&message)
                    && let Some(rid) = field(&message, "requestId")
                    && !seen_js.iter().any(|u| u == &url)
                {
                    seen_js.push(url.clone());
                    LAST_LOCK.lock().unwrap().push((rid, url));
                }
            }
            Some(_) => {
                frames += 1;
            }
            None => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
    eprintln!("harvester: drained frames={frames} js={}", seen_js.len());
    for url in &seen_js {
        println!("JS: {url}");
    }

    // 4. fetch bodies of p-bandai chunk candidates and scan for the minter
    let pairs: Vec<(String, String)> = LAST_LOCK.lock().unwrap().clone();
    for (rid, url) in &pairs {
        if !url.contains("p-bandai.com/assets") {
            continue;
        }
        ws.send_text(&format!(
            r#"{{"id":100,"method":"Network.getResponseBody","params":{{"requestId":"{rid}"}}}}"#
        ));
        if let Some(response) = ws.recv_text() {
            let body = field(&response, "body").unwrap_or_default();
            if body.contains("p8komysnbc") || body.contains("csrfToken") {
                println!("MINTER-CANDIDATE: {url} ({} decoded chars)", body.len());
            }
        }
    }
    ws.send_text(r#"{"id":999,"method":"Network.disable"}"#);
}

static LAST_LOCK: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// The string value of `"key":"..."` at the FIRST occurrence in the message.
fn field(message: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = message.find(&needle)? + needle.len();
    let rest = &message[start..];
    let end = rest.find('"').unwrap_or(rest.len());
    Some(rest[..end].to_owned())
}
