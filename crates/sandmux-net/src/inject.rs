//! Troca da chave falsa pela real: o gateway pede os segredos ao daemon a
//! cada conexão decifrada e troca o falso pelo real nos cabeçalhos dos pedidos
//! ao dono do segredo.

use std::os::unix::net::UnixStream;
use std::time::Duration;

use sandmux_proto::msg::Injection;
use sandmux_proto::{Frame, Msg, PROTOCOL, read_frame, write_control};
use sandmux_spec::VaultAccess;
use sandmux_spec::net::Pattern;

/// Os segredos com domínio, pedidos ao daemon; vazio se o daemon não
/// responde ou o cofre está trancado.
pub fn fetch(access: &VaultAccess, tag: &str) -> Vec<Injection> {
    let Ok(mut s) = UnixStream::connect(&access.socket) else {
        return Vec::new();
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(3)));
    let hello = Msg::Hello {
        protocol: PROTOCOL,
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: access.data_dir.clone(),
        config_dir: access.config_dir.clone(),
        lang: String::new(),
    };
    if write_control(&mut s, &hello).is_err() {
        return Vec::new();
    }
    if !matches!(
        read_frame(&mut s),
        Ok(Some(Frame::Control(Msg::Welcome { .. })))
    ) {
        return Vec::new();
    }
    if write_control(&mut s, &Msg::VaultInject { tag: tag.into() }).is_err() {
        return Vec::new();
    }
    match read_frame(&mut s) {
        Ok(Some(Frame::Control(Msg::Injections { secrets }))) => secrets,
        _ => Vec::new(),
    }
}

/// Só os segredos cujo domínio dono casa com `host`.
pub fn for_host(list: Vec<Injection>, host: &str) -> Vec<Injection> {
    list.into_iter()
        .filter(|i| {
            i.domains
                .iter()
                .filter_map(|d| d.parse::<Pattern>().ok())
                .any(|p| p.matches_name(host))
        })
        .collect()
}

fn replace(text: &str, list: &[Injection], used: &mut Option<String>) -> String {
    let mut out = text.to_string();
    for i in list {
        if !i.fake.is_empty() && out.contains(&i.fake) {
            out = out.replace(&i.fake, &i.value);
            used.get_or_insert_with(|| i.name.clone());
        }
    }
    out
}

/// O base64 de um valor `Basic <base64>` (o esquema sem distinção de
/// maiúsculas).
fn basic_value(v: &str) -> Option<&str> {
    let (scheme, rest) = v.split_at_checked(5)?;
    (scheme.eq_ignore_ascii_case("basic") && rest.starts_with(' ')).then(|| &rest[1..])
}

/// Troca, em cada cabeçalho, o falso de um segredo pelo real. Devolve o nome
/// do (primeiro) segredo usado.
pub fn rewrite_headers(h: &mut hyper::HeaderMap, list: &[Injection]) -> Option<String> {
    let mut used = None;
    for v in h.values_mut() {
        let Ok(text) = v.to_str() else { continue };
        let mut out = replace(text, list, &mut used);
        // `Basic` leva `usuário:senha` em base64: abre, troca e fecha.
        if let Some(enc) = basic_value(&out) {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD;
            if let Ok(plain) = b64.decode(enc.trim())
                && let Ok(plain) = String::from_utf8(plain)
            {
                let swapped = replace(&plain, list, &mut used);
                if swapped != plain {
                    out = format!("{} {}", &out[..5], b64.encode(swapped));
                }
            }
        }
        if out != text
            && let Ok(nv) = hyper::header::HeaderValue::from_str(&out)
        {
            *v = nv;
        }
    }
    used
}

/// Troca o falso pelo real no caminho da URL (o Telegram leva o token em
/// `/bot<token>/`). A query fica como está. Devolve o nome do segredo usado.
pub fn rewrite_path(uri: &mut hyper::Uri, list: &[Injection]) -> Option<String> {
    let path = uri.path();
    let mut used = None;
    let mut out = replace(path, list, &mut used);
    // O `:` do token do Telegram pode vir codificado.
    for i in list {
        let enc = i.fake.replace(':', "%3A");
        if enc != i.fake && !i.fake.is_empty() && out.contains(&enc) {
            out = out.replace(&enc, &i.value);
            used.get_or_insert_with(|| i.name.clone());
        }
    }
    if out == path {
        return None;
    }
    let pq = match uri.query() {
        Some(q) => format!("{out}?{q}"),
        None => out,
    };
    let mut parts = uri.clone().into_parts();
    parts.path_and_query = pq.parse().ok();
    *uri = hyper::Uri::from_parts(parts).ok()?;
    used
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inj(name: &str, domain: &str, fake: &str, value: &str) -> Injection {
        Injection {
            name: name.into(),
            domains: vec![domain.into()],
            fake: fake.into(),
            value: value.into(),
        }
    }

    #[test]
    fn troca_so_para_o_dono() {
        let list = vec![
            inj(
                "openrouter",
                "openrouter.ai",
                "sandmux-fake-aaa",
                "sk-real-1",
            ),
            inj("outro", "*.exemplo.test", "sandmux-fake-bbb", "real-2"),
        ];
        let dono = for_host(list.clone(), "openrouter.ai");
        assert_eq!(dono.len(), 1);
        let mut h = hyper::HeaderMap::new();
        h.insert("authorization", "Bearer sandmux-fake-aaa".parse().unwrap());
        h.insert("x-outro", "sandmux-fake-bbb".parse().unwrap());
        assert_eq!(
            rewrite_headers(&mut h, &dono).as_deref(),
            Some("openrouter")
        );
        assert_eq!(h["authorization"], "Bearer sk-real-1");
        // O falso de outro segredo, que não é deste dono, fica falso.
        assert_eq!(h["x-outro"], "sandmux-fake-bbb");

        // Outro domínio: nada é trocado.
        let outro = for_host(list.clone(), "malicioso.test");
        let mut h2 = hyper::HeaderMap::new();
        h2.insert("authorization", "Bearer sandmux-fake-aaa".parse().unwrap());
        assert_eq!(rewrite_headers(&mut h2, &outro), None);
        assert_eq!(h2["authorization"], "Bearer sandmux-fake-aaa");
        assert_eq!(for_host(list, "api.exemplo.test").len(), 1);
    }

    #[test]
    fn troca_no_caminho() {
        let list = vec![inj(
            "telegram",
            "api.telegram.org",
            "0000000000:sandmux-fake-ddd",
            "123:REAL",
        )];
        let mut u: hyper::Uri = "/bot0000000000:sandmux-fake-ddd/getUpdates?offset=1"
            .parse()
            .unwrap();
        assert_eq!(rewrite_path(&mut u, &list).as_deref(), Some("telegram"));
        assert_eq!(u.to_string(), "/bot123:REAL/getUpdates?offset=1");
        let mut u: hyper::Uri = "/file/bot0000000000%3Asandmux-fake-ddd/a.jpg"
            .parse()
            .unwrap();
        assert_eq!(rewrite_path(&mut u, &list).as_deref(), Some("telegram"));
        assert_eq!(u.path(), "/file/bot123:REAL/a.jpg");
        // Na query, não troca; sem o falso, nada muda.
        let mut u: hyper::Uri = "/x?t=0000000000:sandmux-fake-ddd".parse().unwrap();
        assert_eq!(rewrite_path(&mut u, &list), None);
        assert_eq!(u.to_string(), "/x?t=0000000000:sandmux-fake-ddd");
    }

    #[test]
    fn troca_dentro_do_basic() {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;
        let list = vec![inj("gh", "github.com", "sandmux-fake-ccc", "ghp_real")];
        let mut h = hyper::HeaderMap::new();
        let basic = format!("Basic {}", b64.encode("sandmux:sandmux-fake-ccc"));
        h.insert("authorization", basic.parse().unwrap());
        h.insert(
            "proxy-authorization",
            format!("basic {}", b64.encode("x:y")).parse().unwrap(),
        );
        assert_eq!(rewrite_headers(&mut h, &list).as_deref(), Some("gh"));
        assert_eq!(
            h["authorization"],
            format!("Basic {}", b64.encode("sandmux:ghp_real")).as_str()
        );
        // Sem o falso, o `Basic` fica como estava.
        assert_eq!(
            h["proxy-authorization"],
            format!("basic {}", b64.encode("x:y")).as_str()
        );
        // Base64 inválido: intacto.
        let mut h2 = hyper::HeaderMap::new();
        h2.insert("authorization", "Basic !!!".parse().unwrap());
        assert_eq!(rewrite_headers(&mut h2, &list), None);
        assert_eq!(h2["authorization"], "Basic !!!");
    }
}
