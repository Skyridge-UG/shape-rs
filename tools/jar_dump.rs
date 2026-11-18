//! CDP Network.getCookies: the browser's exact jar for p-bandai.com.
use std::io::{Read, Write};
use std::net::TcpStream;

fn main() {
    let ws_url = std::env::args().nth(1).unwrap_or_else(|| {
        "ws://127.0.0.1:9222/devtools/page/EAEF0E84243BAA900706327264E0D461".into()
    });
    let authority = ws_url.trim_start_matches("ws://");
    let (hostport, path) = authority.split_once('/').unwrap();
    let mut s = TcpStream::connect(hostport).unwrap();
    let hs = format!(
        "GET /{} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
        path, hostport
    );
    s.write_all(hs.as_bytes()).unwrap();
    let mut buf = [0u8; 16384];
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

    let _send = |s: &mut TcpStream, text: &str| {
        let p = text.as_bytes();
        let l = p.len();
        let mut frame = if l < 126 {
            vec![0x81, l as u8 | 0x80]
        } else {
            let mut h = vec![0x81, 126 | 0x80];
            h.extend_from_slice(&(l as u16).to_be_bytes());
            h
        };
        frame.extend_from_slice(&[0u8; 4]);
        frame.extend_from_slice(p);
        s.write_all(&frame).unwrap();
    };

    // Network.enable then Network.getCookies with the bandai URL
    let req1 = r#"{"id":1,"method":"Network.enable"}"#;
    let l1 = req1.len();
    let mut f1 = if l1 < 126 {
        vec![0x81, l1 as u8 | 0x80]
    } else {
        vec![0x81, 126 | 0x80, (l1 >> 8) as u8, (l1 & 0xff) as u8]
    };
    f1.extend_from_slice(&[0u8; 4]);
    f1.extend_from_slice(req1.as_bytes());
    s.write_all(&f1).unwrap();

    let req2 = r#"{"id":2,"method":"Network.getCookies","params":{"urls":["https://p-bandai.com","https://p-bandai.com/us"]}}"#;
    let l2 = req2.len();
    let mut f2 = if l2 < 126 {
        vec![0x81, l2 as u8 | 0x80]
    } else {
        vec![0x81, 126 | 0x80, (l2 >> 8) as u8, (l2 & 0xff) as u8]
    };
    f2.extend_from_slice(&[0u8; 4]);
    f2.extend_from_slice(req2.as_bytes());
    s.write_all(&f2).unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let mut head = [0u8; 2];
        if s.read_exact(&mut head).is_err() {
            break;
        }
        let len = match head[1] & 0x7f {
            126 => {
                let mut e = [0u8; 2];
                s.read_exact(&mut e).unwrap();
                u16::from_be_bytes(e) as usize
            }
            127 => {
                let mut e = [0u8; 8];
                s.read_exact(&mut e).unwrap();
                u64::from_be_bytes(e) as usize
            }
            n => n as usize,
        };
        let mut p = vec![0u8; len];
        if len > 0 {
            s.read_exact(&mut p).unwrap();
        }
        let message = String::from_utf8_lossy(&p).into_owned();
        if message.contains("\"id\":2") {
            // print every cookie name=value pair
            let mut search = 0usize;
            while let Some(rel) = message[search..].find("\"name\":\"") {
                let nstart = search + rel + 8;
                let nend = message[nstart..].find('"').unwrap() + nstart;
                let name = &message[nstart..nend];
                let vkey = "\"value\":\"";
                let vstart_rel = message[nend..].find(vkey).unwrap() + vkey.len();
                let vstart = nend + vstart_rel;
                let vend = message[vstart..].find('"').unwrap() + vstart;
                let value = &message[vstart..vend];
                println!("{}={}", name, &value[..value.len().min(60)]);
                search = vend;
            }
            break;
        }
    }
}
