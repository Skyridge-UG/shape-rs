//! addSnippet + reload + auto-login + header capture in ONE CDP session.
//! The page's own JS mints p8komysnbc at load; the snippet logs every
//! setRequestHeader AND fetch with p8k headers across the reload, then
//! auto-fills and clicks Sign In. Full truth capture, hands-off.

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
    s
}

fn send(ws: &mut TcpStream, text: &str) {
    let p = text.as_bytes();
    let l = p.len();
    let mut frame = if l < 126 {
        vec![0x81, l as u8 | 0x80]
    } else if l < 65_536 {
        let mut h = vec![0x81, 126 | 0x80];
        h.extend_from_slice(&(l as u16).to_be_bytes());
        h
    } else {
        let mut h = vec![0x81, 127 | 0x80];
        h.extend_from_slice(&(l as u64).to_be_bytes());
        h
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

fn evaluate(ws: &mut TcpStream, id: u32, expr: &str, await_promise: bool) -> Option<String> {
    let escaped = expr
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    let ap = if await_promise {
        ",\"awaitPromise\":true"
    } else {
        ""
    };
    send(
        ws,
        &format!(r#"{{"id":{id},"method":"Runtime.addBinding"}}"#,),
    );
    send(
        ws,
        &format!(
            r#"{{"id":{id}00,"method":"Runtime.evaluate","params":{{"expression":"{escaped}"{ap},"returnByValue":true}}}}"#
        ),
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(40);
    let mut result = None;
    while std::time::Instant::now() < deadline {
        if let Some(message) = recv(ws) {
            if message.contains(&format!("\"id\":{}00", id)) {
                let key = "\"value\":\"";
                if let Some(start) = message.find(key) {
                    let rest = &message[start + key.len()..];
                    let end = rest.find('"').unwrap_or(rest.len());
                    result = Some(rest[..end].to_owned());
                }
                if !await_promise {
                    break;
                }
            }
            // capture binding events (the snippet streams headers to us)
            if message.contains("snippetHeader")
                && let Some(payload) = message.split("\"payload\":\"").nth(1)
            {
                let end = payload.find('"').unwrap_or(payload.len());
                println!("HDR-EVENT: {}", &payload[..end]);
            }
        }
    }
    result
}

fn main() {
    let ws_url = std::env::args().nth(1).unwrap_or_else(|| {
        "ws://127.0.0.1:9222/devtools/page/EAEF0E84243BAA900706327264E0D461".into()
    });
    let mut ws = ws_connect(&ws_url);

    // register a binding for the snippet to stream headers back
    send(
        &mut ws,
        r#"{"id":1,"method":"Runtime.addBinding","params":{"name":"snippetHeader"}}"#,
    );
    let _ = recv(&mut ws);

    // the snippet: hook XHR + fetch for p8k headers, survives reload
    let snippet = r#"
        window.__cap = [];
        const rec = (tag, obj) => { try { window.__cap.push([tag, JSON.stringify(obj).slice(0, 400)]); snippetHeader(JSON.stringify({tag, data: JSON.stringify(obj).slice(0, 400)})); } catch(e) {} };
        const origX = XMLHttpRequest.prototype.setRequestHeader;
        XMLHttpRequest.prototype.setRequestHeader = function(k, v) {
            if (String(k).startsWith('p8k')) rec('xhr', {k, v: String(v).slice(0, 120)});
            return origX.call(this, k, v);
        };
        const origF = window.fetch;
        window.fetch = function(u, o) {
            o = o || {};
            const H = o.headers || {};
            const get = (n) => H[n] !== undefined ? H[n] : (H instanceof Headers && H.get ? H.get(n) : undefined);
            if (get('p8komysnbc-a') || get('p8kOmysnbc-a')) rec('fetch', {url: String(u).slice(0,80), a: get('p8komysnbc-a') || get('p8kOmysnbc-a')});
            return origF.call(this, u, o);
        };
        'hooks-installed';
    "#;
    evaluate(&mut ws, 2, snippet, false);

    // auto-fill + auto-click after load completes
    let auto = r#"
        window.__autoLogin = () => {
            const em = document.querySelector('input[name=e-mail_address]');
            const pw = document.querySelector('input[name=password]');
            if (em) { em.value = 'billymarquez@onezeromail.com'; em.dispatchEvent(new Event('input', {bubbles:true})); }
            if (pw) { pw.value = 'D75ovL3j!gg8EK_'; pw.dispatchEvent(new Event('input', {bubbles:true})); }
            setTimeout(() => {
                const btn = [...document.querySelectorAll('button')].find(b => b.textContent.trim() === 'Sign In' && b.className.includes('p-button'));
                if (btn) { rec('click', {}); btn.click(); }
            }, 2500);
        };
        if (document.readyState === 'complete') window.__autoLogin(); else window.addEventListener('load', () => window.__autoLogin());
        'auto armed';
    "#;
    evaluate(&mut ws, 3, auto, false);

    // reload - snippet re-installs via addScriptToEvaluateOnNewDocument? We
    // used evaluate, so instead: navigate then re-arm + auto login.
    send(&mut ws, r#"{"id":4,"method":"Page.enable"}"#);
    let _ = recv(&mut ws);
    send(
        &mut ws,
        r#"{"id":5,"method":"Page.reload","params":{"ignoreCache":true}}"#,
    );
    std::thread::sleep(Duration::from_secs(12));

    // re-arm hooks + auto login post-reload
    evaluate(&mut ws, 6, snippet, false);
    evaluate(&mut ws, 7, auto, true);

    // drain results
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if let Some(message) = recv(&mut ws)
            && message.contains("snippetHeader")
            && let Some(payload) = message.split("\"payload\":\"").nth(1)
        {
            let end = payload.find('"').unwrap_or(payload.len());
            println!("HDR: {}", payload[..end].replace("\\\"", "\""));
        }
    }
    println!("capture complete");
}
