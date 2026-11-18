//! CDP evaluate: run JS in the live bandai tab and print the result.
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

fn field(message: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = message.find(&needle)? + needle.len();
    let rest = &message[start..];
    let end = rest.find('"').unwrap_or(rest.len());
    Some(rest[..end].to_owned())
}

fn main() {
    let ws_url = std::env::args().nth(1).expect("ws url");
    let expr = std::env::args().nth(2).expect("js expression");
    let mut ws = ws_connect(&ws_url);
    send(&mut ws, r#"{"id":1,"method":"Runtime.enable"}"#);
    let _ = recv(&mut ws);
    let escaped = expr
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    let call = format!(
        "{{\"id\":2,\"method\":\"Runtime.evaluate\",\"params\":{{\"expression\":\"{}\",\"returnByValue\":true}}}}",
        escaped
    );
    send(&mut ws, &call);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if let Some(message) = recv(&mut ws)
            && message.contains("\"id\":2")
        {
            if let Some(value) = field(&message, "value") {
                println!("{}", value.replace("\\n", "\n").replace("\\\"", "\""));
            }
            break;
        }
    }
}
