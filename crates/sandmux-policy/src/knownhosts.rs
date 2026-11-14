//! O `~/.ssh/known_hosts` de uma sessão com o agente SSH: o guardado no HOME
//! sintético mais as linhas do host para os servidores das chaves.

use hmac::{Hmac, Mac};

/// Se `pattern` (um nome da lista de uma linha do `known_hosts`) casa com
/// `server`: nome simples, `[nome]:porta`, curinga `*`/`?` ou em hash
/// (`|1|sal|hash`).
fn host_matches(pattern: &str, server: &str) -> bool {
    if let Some(rest) = pattern.strip_prefix("|1|") {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;
        let Some((salt, hash)) = rest.split_once('|') else {
            return false;
        };
        let (Ok(salt), Ok(hash)) = (b64.decode(salt), b64.decode(hash)) else {
            return false;
        };
        let Ok(mac) = Hmac::<sha1::Sha1>::new_from_slice(&salt) else {
            return false;
        };
        let check = |name: &str| {
            let mut m = mac.clone();
            m.update(name.as_bytes());
            m.verify_slice(&hash).is_ok()
        };
        return check(server);
    }
    let name = pattern
        .strip_prefix('[')
        .and_then(|p| p.split_once("]:").map(|(h, _)| h))
        .unwrap_or(pattern);
    glob(&name.to_ascii_lowercase(), &server.to_ascii_lowercase())
}

fn glob(p: &str, s: &str) -> bool {
    match (p.chars().next(), s.chars().next()) {
        (None, None) => true,
        (Some('*'), _) => {
            glob(&p[1..], s)
                || (!s.is_empty() && glob(p, &s[s.chars().next().map_or(0, char::len_utf8)..]))
        }
        (Some('?'), Some(c)) => glob(&p[1..], &s[c.len_utf8()..]),
        (Some(a), Some(b)) if a == b => glob(&p[a.len_utf8()..], &s[b.len_utf8()..]),
        _ => false,
    }
}

/// As linhas de `host_text` cujos nomes casam com algum de `servers`.
/// Negações (`!nome`) tiram a linha.
pub fn lines_for(host_text: &str, servers: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for line in host_text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let mut fields = t.split_whitespace();
        let mut first = fields.next().unwrap_or_default();
        if first.starts_with('@') {
            first = fields.next().unwrap_or_default();
        }
        let pats: Vec<&str> = first.split(',').collect();
        let hit = servers.iter().any(|s| {
            let neg = pats
                .iter()
                .filter_map(|p| p.strip_prefix('!'))
                .any(|p| host_matches(p, s));
            !neg && pats
                .iter()
                .filter(|p| !p.starts_with('!'))
                .any(|p| host_matches(p, s))
        });
        if hit {
            out.push(t.to_string());
        }
    }
    out
}

/// O arquivo da sessão: o guardado mais as linhas do host que faltam nele.
pub fn session_file(saved: &str, host_text: &str, servers: &[String]) -> String {
    let mut text = saved.to_string();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    for l in lines_for(host_text, servers) {
        if !saved.lines().any(|s| s.trim() == l) {
            text.push_str(&l);
            text.push('\n');
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hashed(name: &str) -> String {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;
        let salt = b"0123456789abcdefghij";
        let mut m = Hmac::<sha1::Sha1>::new_from_slice(salt).unwrap();
        m.update(name.as_bytes());
        format!(
            "|1|{}|{}",
            b64.encode(salt),
            b64.encode(m.finalize().into_bytes())
        )
    }

    #[test]
    fn casa_os_servidores_das_chaves() {
        let host = format!(
            "# comentário\n\
             github.com ssh-ed25519 AAAA1\n\
             gitlab.com,1.2.3.4 ssh-ed25519 AAAA2\n\
             [git.empresa.com]:2222 ssh-ed25519 AAAA3\n\
             {} ssh-ed25519 AAAA4\n\
             {} ssh-ed25519 AAAA5\n\
             @cert-authority *.empresa.com ssh-ed25519 AAAA6\n\
             *.empresa.com,!git.empresa.com ssh-ed25519 AAAA7\n\
             outro.com ssh-ed25519 AAAA8\n",
            hashed("github.com"),
            hashed("nao.com")
        );
        let servers = vec!["github.com".to_string(), "git.empresa.com".to_string()];
        let got = lines_for(&host, &servers);
        let keys: Vec<&str> = got.iter().map(|l| l.rsplit(' ').next().unwrap()).collect();
        assert_eq!(keys, vec!["AAAA1", "AAAA3", "AAAA4", "AAAA6"]);
        let f = session_file("github.com ssh-ed25519 AAAA1", &host, &servers);
        assert_eq!(f.matches("AAAA1").count(), 1);
        assert!(f.ends_with("AAAA6\n"));
        assert_eq!(session_file("", "", &servers), "");
    }

    #[test]
    fn curingas() {
        assert!(glob("*.x.com", "a.x.com"));
        assert!(!glob("*.x.com", "x.com"));
        assert!(glob("h?st", "host"));
        assert!(glob("github.com", "github.com"));
    }
}
