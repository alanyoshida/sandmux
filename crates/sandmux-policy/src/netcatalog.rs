//! Catálogos de rede embutidos, versionados com o binário. O sandmux nunca
//! os atualiza pela rede. O conteúdo foi revisado pelo usuário em 2026-11-14.

/// Destinos de exfiltração, negados por padrão (desligáveis com
/// `catalog-off`), por categoria.
pub const EXFIL: &[(&str, &[&str])] = &[
    (
        "colagem de texto",
        &[
            "pastebin.com",
            "paste.ee",
            "dpaste.com",
            "dpaste.org",
            "paste.rs",
            "termbin.com",
            "ix.io",
            "sprunge.us",
            "hastebin.com",
            "rentry.co",
            "controlc.com",
        ],
    ),
    (
        "envio de arquivos",
        &[
            "transfer.sh",
            "0x0.st",
            "file.io",
            "tmpfiles.org",
            "catbox.moe",
            "litter.catbox.moe",
            "gofile.io",
            "bashupload.com",
            "temp.sh",
            "oshi.at",
            "wetransfer.com",
            "*.wetransfer.com",
            "*.sendspace.com",
        ],
    ),
    (
        "coletores de requisição",
        &[
            "webhook.site",
            "*.webhook.site",
            "requestbin.net",
            "*.m.pipedream.net",
            "*.free.beeceptor.com",
            "requestcatcher.com",
            "*.requestcatcher.com",
        ],
    ),
    (
        "OAST e DNS de exfiltração",
        &[
            "*.oast.fun",
            "*.oast.live",
            "*.oast.pro",
            "*.oast.site",
            "*.oast.online",
            "*.oast.me",
            "*.oastify.com",
            "*.burpcollaborator.net",
            "*.dnslog.cn",
            "*.ceye.io",
            "*.interact.sh",
        ],
    ),
    (
        "túneis",
        &[
            "*.ngrok.io",
            "*.ngrok-free.app",
            "*.ngrok.app",
            "*.loca.lt",
            "*.trycloudflare.com",
            "serveo.net",
            "*.serveo.net",
            "*.localhost.run",
            "*.pinggy.link",
        ],
    ),
];

/// Domínios comuns, desligados por padrão, em grupos ligados por `catalog`.
pub const COMMON: &[(&str, &[&str])] = &[
    (
        "github",
        &[
            "github.com",
            "*.github.com",
            "*.githubusercontent.com",
            "ghcr.io",
        ],
    ),
    ("gitlab", &["gitlab.com", "*.gitlab.com"]),
    ("npm", &["registry.npmjs.org", "registry.yarnpkg.com"]),
    ("pypi", &["pypi.org", "files.pythonhosted.org"]),
    (
        "crates",
        &["crates.io", "index.crates.io", "static.crates.io"],
    ),
    ("go", &["proxy.golang.org", "sum.golang.org"]),
    ("rubygems", &["rubygems.org", "index.rubygems.org"]),
    ("maven", &["repo1.maven.org", "repo.maven.apache.org"]),
    (
        "docker",
        &[
            "registry-1.docker.io",
            "auth.docker.io",
            "production.cloudflare.docker.com",
        ],
    ),
    ("node", &["nodejs.org"]),
    ("rust", &["static.rust-lang.org"]),
    ("opencode", &["opencode.ai", "models.opencode.ai"]),
];

/// Provedores de inferência do modo só inferência (desligáveis com
/// `catalog-off`).
pub const INFERENCE: &[&str] = &[
    "openrouter.ai",
    "api.anthropic.com",
    "api.openai.com",
    "generativelanguage.googleapis.com",
    "api.deepseek.com",
    "api.mistral.ai",
    "api.groq.com",
    "api.x.ai",
];

/// Origem dos itens de cada catálogo, como aparece nas regras.
pub const EXFIL_ORIGIN: &str = "catálogo de exfiltração";
pub const INFERENCE_ORIGIN: &str = "catálogo de inferência";

pub fn common_origin(group: &str) -> String {
    format!("catálogo comum ({group})")
}

/// O grupo existe no catálogo comum?
pub fn is_group(name: &str) -> bool {
    COMMON.iter().any(|(g, _)| *g == name)
}

/// Nomes dos grupos, para as mensagens.
pub fn group_names() -> String {
    COMMON
        .iter()
        .map(|(g, _)| *g)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sandmux_spec::net::Pattern;

    #[test]
    fn todos_os_itens_sao_padroes_validos_e_sem_repeticao() {
        let mut seen = std::collections::HashSet::new();
        let all = EXFIL
            .iter()
            .flat_map(|(_, v)| v.iter())
            .chain(COMMON.iter().flat_map(|(_, v)| v.iter()))
            .chain(INFERENCE.iter());
        for i in all {
            let p: Pattern = i.parse().unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(p.to_string(), *i);
            assert!(seen.insert(*i), "repetido: {i}");
        }
        assert!(is_group("npm") && !is_group("nada"));
        assert!(group_names().starts_with("github, gitlab"));
    }
}
