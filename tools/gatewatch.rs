//! Gate watch: cycles the proxied login until non-503, then reports PASS.
//! The one-shot gate watcher for the current session.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

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
    let cycles: u32 = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(8);
    let wait_secs: u64 = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(240);
    let ws_url = std::env::args().nth(3).unwrap_or_else(|| {
        "ws://127.0.0.1:9222/devtools/page/EAEF0E84243BAA900706327264E0D461".into()
    });

    for cycle in 1..=cycles {
        let mut ws = ws_connect(&ws_url);
        let status = evaluate(
            &mut ws,
            cycle,
            "fetch('/login',{method:'POST',headers:{'content-type':'application/x-www-form-urlencoded','x-requested-with':'XMLHttpRequest'},body:'probe=1',credentials:'include'}).then(r=>r.status).catch(e=>0)",
        )
        .unwrap_or_default();
        println!("cycle {cycle}: gate={}", status.trim());
        drop(ws);

        let code: u16 = status.trim().parse().unwrap_or(0);
        if code != 0 && code != 503 {
            println!("GATE OPEN (={code}) - run fullcircle-proxy now");
            std::process::exit(0);
        }
        if cycle < cycles {
            std::thread::sleep(Duration::from_secs(wait_secs));
        }
    }
    println!("gate closed through all {cycles} cycles");
}
