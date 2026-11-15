//! Descoberta dos modelos de um servidor local compatível com a OpenAI:
//! `GET <base>/models`, feito pelo daemon fora do sandbox, com tempo curto.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

/// Tempo para conectar (um servidor fora do ar recusa na hora).
const CONNECT: Duration = Duration::from_millis(1500);
/// Tempo para a resposta: o Unsloth Studio leva perto de 1,6 s para listar
/// os modelos, porque percorre a pasta deles.
const RESPONSE: Duration = Duration::from_secs(6);
/// Maior resposta aceita.
const MAX: usize = 1 << 20;

/// Decodifica um corpo `Transfer-Encoding: chunked`.
fn unchunk(mut body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let i = body.windows(2).position(|w| w == b"\r\n")?;
        let size = std::str::from_utf8(&body[..i]).ok()?;
        let size = usize::from_str_radix(size.split(';').next()?.trim(), 16).ok()?;
        body = &body[i + 2..];
        if size == 0 {
            return Some(out);
        }
        if body.len() < size + 2 {
            return None;
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

/// Os ids de `data[].id` de uma resposta de `/models`.
pub fn ids_of(json: &[u8]) -> Vec<String> {
    serde_json::from_slice::<serde_json::Value>(json)
        .ok()
        .and_then(|v| v.get("data").and_then(|d| d.as_array()).cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(str::to_string))
        .collect()
}

/// Pede `GET <base_url>/models` no `127.0.0.1:port` do host. `None` quando
/// o servidor não responde, recusa ou devolve algo que não é a lista.
pub fn models(base_url: &str, port: u16, authorization: Option<&str>) -> Option<Vec<String>> {
    let path = base_url
        .strip_prefix("http://")?
        .split_once('/')
        .map(|(_, p)| format!("/{}", p.trim_end_matches('/')))
        .unwrap_or_default();
    let mut s =
        TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), CONNECT).ok()?;
    s.set_read_timeout(Some(RESPONSE)).ok()?;
    s.set_write_timeout(Some(CONNECT)).ok()?;
    let mut req = format!(
        "GET {path}/models HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: application/json\r\nUser-Agent: sandmux\r\nConnection: close\r\n"
    );
    if let Some(a) = authorization {
        req.push_str(&format!("Authorization: {a}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).ok()?;
    let mut resp = Vec::new();
    let mut buf = [0u8; 16384];
    loop {
        match s.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                resp.extend_from_slice(&buf[..n]);
                if resp.len() > MAX {
                    return None;
                }
            }
            Err(_) => break,
        }
    }
    let end = resp.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&resp[..end]).to_ascii_lowercase();
    if !head.starts_with("http/1.1 200") && !head.starts_with("http/1.0 200") {
        return None;
    }
    let body = &resp[end + 4..];
    let body = if head.contains("transfer-encoding: chunked") {
        unchunk(body)?
    } else {
        body.to_vec()
    };
    let ids = ids_of(&body);
    (!ids.is_empty()).then_some(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(resp: &'static str) -> (u16, std::thread::JoinHandle<String>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut buf = [0u8; 2048];
            let n = c.read(&mut buf).unwrap();
            c.write_all(resp.as_bytes()).unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        (port, h)
    }

    #[test]
    fn lista_com_tamanho_e_chave() {
        let (port, h) = server(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 46\r\n\r\n{\"data\":[{\"id\":\"Bonsai-27B-GGUF\"},{\"id\":\"g\"}]}",
        );
        let m = models(
            &format!("http://127.0.0.1:{port}/v1"),
            port,
            Some("Bearer real"),
        )
        .unwrap();
        assert_eq!(m, vec!["Bonsai-27B-GGUF", "g"]);
        let req = h.join().unwrap();
        assert!(req.starts_with("GET /v1/models HTTP/1.1"), "{req}");
        assert!(req.contains("Authorization: Bearer real"), "{req}");
    }

    #[test]
    fn servidor_lento_como_o_unsloth() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut buf = [0u8; 2048];
            let _ = c.read(&mut buf);
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let _ = c.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-length: 24\r\n\r\n{\"data\":[{\"id\":\"qwen\"}]}",
            );
        });
        assert_eq!(
            models(&format!("http://127.0.0.1:{port}/v1"), port, None).unwrap(),
            vec!["qwen"]
        );
    }

    #[test]
    fn lista_em_pedacos_e_recusa() {
        let (port, _h) = server(
            "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\nf\r\n{\"data\":[{\"id\":\r\n9\r\n\"llama3\"}\r\n2\r\n]}\r\n0\r\n\r\n",
        );
        assert_eq!(
            models(&format!("http://127.0.0.1:{port}/v1"), port, None).unwrap(),
            vec!["llama3"]
        );
        let (port, _h) = server("HTTP/1.1 401 Unauthorized\r\ncontent-length: 2\r\n\r\n{}");
        assert_eq!(
            models(&format!("http://127.0.0.1:{port}/v1"), port, None),
            None
        );
        // Ninguém escutando.
        let free = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        assert_eq!(models("http://127.0.0.1:1/v1", free, None), None);
    }
}
