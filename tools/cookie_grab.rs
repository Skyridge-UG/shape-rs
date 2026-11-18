//! CDP cookies: dump every cookie (incl. httpOnly) for the bandai tab as
//! name=value lines - the jar seed for external transports.
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

/// Pull `"name":"<v>"` pairs from the Network.getCookies reply. Cookie values
/// may contain quotes, so scan pair-wise instead of one flat field grab.
fn cookie_pairs(reply: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = reply;
    while let Some(pos) = rest.find("\"name\":") {
        rest = &rest[pos + 7..];
        let name = quoted(rest);
        let Some(after) = rest.find("\"value\":") else {
            break;
        };
        let value = quoted(&rest[after + 8..]);
        if !name.is_empty() {
            out.push((name, value));
        }
        rest = &rest[after + 8..];
    }
    out
}

fn quoted(s: &str) -> String {
    let s = s.trim_start();
    let s = s.strip_prefix('"').unwrap_or(s);
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            '"' => break,
            _ => out.push(c),
        }
    }
    out
}

fn main() {
    let ws_url = std::env::args().nth(1).expect("ws url (page target)");
    let url_filter = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "p-bandai.com".into());
    let mut ws = ws_connect(&ws_url);
    send(&mut ws, r#"{"id":1,"method":"Network.enable"}"#);
    let _ = recv(&mut ws);
    let call = format!(
        r#"{{"id":2,"method":"Network.getCookies","params":{{"urls":["https://{url_filter}/"]}}}}"#
    );
    send(&mut ws, &call);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if let Some(message) = recv(&mut ws)
            && message.contains("\"id\":2")
        {
            for (name, value) in cookie_pairs(&message) {
                println!("{name}={value}");
            }
            break;
        }
    }
}
