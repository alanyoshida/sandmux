//! Domínio a partir do primeiro pedaço de uma conexão: SNI do TLS ou `Host`
//! do HTTP. Só lê; nada do conteúdo é guardado.

/// SNI de um ClientHello do TLS.
pub fn tls_sni(b: &[u8]) -> Option<String> {
    let data = extension(b, 0)?;
    // server_name: lista (2), tipo (1), tamanho (2), nome.
    let n = u16::from_be_bytes([*data.get(3)?, *data.get(4)?]) as usize;
    let name = data.get(5..5 + n)?;
    std::str::from_utf8(name)
        .ok()
        .map(|s| s.to_ascii_lowercase())
}

/// Os protocolos que o ClientHello oferece (ALPN: `h2`, `http/1.1`), na
/// ordem do cliente; vazio sem a extensão.
pub fn tls_alpn(b: &[u8]) -> Vec<String> {
    let Some(data) = extension(b, 16) else {
        return Vec::new();
    };
    // Lista (2) e, em cada item, tamanho (1) e nome.
    let mut out = Vec::new();
    let mut p = 2;
    while let Some(&n) = data.get(p) {
        let Some(name) = data.get(p + 1..p + 1 + n as usize) else {
            break;
        };
        out.push(String::from_utf8_lossy(name).into_owned());
        p += 1 + n as usize;
    }
    out
}

/// Os dados de uma extensão do ClientHello.
fn extension(b: &[u8], kind: u16) -> Option<&[u8]> {
    // Registro TLS: tipo 22 (handshake), versão, tamanho.
    if b.len() < 5 || b[0] != 22 {
        return None;
    }
    let rec = b.get(5..)?;
    // Handshake: tipo 1 (ClientHello), tamanho de 3 bytes.
    if *rec.first()? != 1 {
        return None;
    }
    let mut p = 4 + 2 + 32; // tipo+tamanho, versão, random
    let sid = *rec.get(p)? as usize;
    p += 1 + sid;
    let cs = u16::from_be_bytes([*rec.get(p)?, *rec.get(p + 1)?]) as usize;
    p += 2 + cs;
    let cm = *rec.get(p)? as usize;
    p += 1 + cm;
    let ext_len = u16::from_be_bytes([*rec.get(p)?, *rec.get(p + 1)?]) as usize;
    p += 2;
    let end = (p + ext_len).min(rec.len());
    while p + 4 <= end {
        let et = u16::from_be_bytes([rec[p], rec[p + 1]]);
        let el = u16::from_be_bytes([rec[p + 2], rec[p + 3]]) as usize;
        let data = rec.get(p + 4..p + 4 + el)?;
        if et == kind {
            return Some(data);
        }
        p += 4 + el;
    }
    None
}

/// `Host` de um pedido HTTP/1.
pub fn http_host(b: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(&b[..b.len().min(4096)]).ok()?;
    let mut lines = text.split("\r\n");
    let first = lines.next()?;
    let method = first.split(' ').next()?;
    if !method.chars().all(|c| c.is_ascii_uppercase()) || method.is_empty() {
        return None;
    }
    lines
        .take_while(|l| !l.is_empty())
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case("host").then(|| v.trim().to_string())
        })
        .map(|h| {
            // Tira a porta.
            h.rsplit_once(':')
                .filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit()))
                .map(|(n, _)| n.to_string())
                .unwrap_or(h)
                .to_ascii_lowercase()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_hello(name: &str) -> Vec<u8> {
        let mut sni = vec![0, 0]; // tipo de extensão: server_name
        let list_len = 3 + name.len();
        let ext_len = 2 + list_len;
        sni.extend_from_slice(&(ext_len as u16).to_be_bytes());
        sni.extend_from_slice(&(list_len as u16).to_be_bytes());
        sni.push(0);
        sni.extend_from_slice(&(name.len() as u16).to_be_bytes());
        sni.extend_from_slice(name.as_bytes());
        let mut body = vec![3, 3];
        body.extend_from_slice(&[0; 32]);
        body.push(0); // session id
        body.extend_from_slice(&[0, 2, 0x13, 0x01]); // cipher suites
        body.extend_from_slice(&[1, 0]); // compression
        body.extend_from_slice(&(sni.len() as u16).to_be_bytes());
        body.extend_from_slice(&sni);
        let mut hs = vec![1, 0, (body.len() >> 8) as u8, body.len() as u8];
        hs.extend_from_slice(&body);
        let mut rec = vec![22, 3, 1, (hs.len() >> 8) as u8, hs.len() as u8];
        rec.extend_from_slice(&hs);
        rec
    }

    #[test]
    fn sni_e_host() {
        assert_eq!(
            tls_sni(&client_hello("API.Exemplo.test")).as_deref(),
            Some("api.exemplo.test")
        );
        assert_eq!(tls_sni(b"GET / HTTP/1.1\r\n"), None);
        assert_eq!(
            http_host(b"GET /x HTTP/1.1\r\nUser-Agent: c\r\nHost: Exemplo.test:8080\r\n\r\n")
                .as_deref(),
            Some("exemplo.test")
        );
        assert_eq!(http_host(b"\x16\x03\x01"), None);
        assert_eq!(http_host(b"GET / HTTP/1.1\r\n\r\n"), None);
    }

    /// Um ClientHello de verdade, do `rustls`, com o ALPN dado.
    fn rustls_hello(alpn: &[&str]) -> Vec<u8> {
        let mut cfg = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();
        cfg.alpn_protocols = alpn.iter().map(|a| a.as_bytes().to_vec()).collect();
        let mut c = rustls::ClientConnection::new(
            std::sync::Arc::new(cfg),
            "api.exemplo.test".try_into().unwrap(),
        )
        .unwrap();
        let mut out = Vec::new();
        c.write_tls(&mut out).unwrap();
        out
    }

    #[test]
    fn alpn_do_client_hello() {
        let h = rustls_hello(&["h2", "http/1.1"]);
        assert_eq!(tls_alpn(&h), vec!["h2", "http/1.1"]);
        assert_eq!(tls_sni(&h).as_deref(), Some("api.exemplo.test"));
        assert!(tls_alpn(&rustls_hello(&[])).is_empty());
        assert!(tls_alpn(&client_hello("a.test")).is_empty());
        assert!(tls_alpn(b"GET / HTTP/1.1\r\n").is_empty());
    }
}
