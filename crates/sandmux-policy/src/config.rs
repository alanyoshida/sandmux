//! Arquivo de configuração (`config.toml`) e camadas de política.
//!
//! O arquivo guarda só a diferença em relação às camadas embutidas, em três
//! tipos de seção: `[global]`, `[profile.<nome>]` e `[project."<pasta>"]`.
//! [`resolve`] combina baseline, preset, global, perfil nomeado, perfil do
//! projeto e opções do `run` numa política efetiva, guardando a origem de
//! cada valor.

pub mod edit;

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use toml_edit::{Document, Item, Value};

use crate::compose::{LayerMode, RunOptions};
use crate::host::Host;

/// Nome do arquivo dentro da pasta de configuração.
pub const FILE: &str = "config.toml";

/// Chaves de lista, na ordem em que aparecem nas mensagens.
pub const LIST_KEYS: &[&str] = &["ro", "rw", "pass-env"];
/// Chaves de liga e desliga.
pub const BOOL_KEYS: &[&str] = &["repo", "kvm", "gpu", "clipboard"];

/// Caminho do arquivo de configuração do host.
pub fn path(host: &Host) -> PathBuf {
    host.config_dir.join(FILE)
}

/// Erro de leitura ou de validação, com a linha quando se sabe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub message: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(l) => write!(
                f,
                "configuração inválida em {}, linha {l}: {}",
                self.path.display(),
                self.message
            ),
            None => write!(
                f,
                "configuração inválida em {}: {}",
                self.path.display(),
                self.message
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Uma camada: o que uma seção (ou as opções do `run`) define.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layer {
    pub ro: Vec<PathBuf>,
    pub rw: Vec<PathBuf>,
    pub pass_env: Vec<String>,
    pub repo: Option<bool>,
    pub kvm: Option<bool>,
    pub gpu: Option<bool>,
    pub clipboard: Option<bool>,
}

impl Layer {
    pub fn is_empty(&self) -> bool {
        *self == Layer::default()
    }

    fn get_bool(&self, key: &str) -> Option<bool> {
        match key {
            "repo" => self.repo,
            "kvm" => self.kvm,
            "gpu" => self.gpu,
            "clipboard" => self.clipboard,
            _ => None,
        }
    }

    fn bool_mut(&mut self, key: &str) -> Option<&mut Option<bool>> {
        match key {
            "repo" => Some(&mut self.repo),
            "kvm" => Some(&mut self.kvm),
            "gpu" => Some(&mut self.gpu),
            "clipboard" => Some(&mut self.clipboard),
            _ => None,
        }
    }

    /// As chaves definidas, no formato do TOML, para listagens.
    pub fn describe(&self) -> Vec<String> {
        let mut out = Vec::new();
        let list = |v: &[String]| {
            v.iter()
                .map(|s| format!("{s:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let paths = |v: &[PathBuf]| {
            v.iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        };
        if !self.ro.is_empty() {
            out.push(format!("ro = [{}]", list(&paths(&self.ro))));
        }
        if !self.rw.is_empty() {
            out.push(format!("rw = [{}]", list(&paths(&self.rw))));
        }
        if !self.pass_env.is_empty() {
            out.push(format!("pass-env = [{}]", list(&self.pass_env)));
        }
        for k in BOOL_KEYS {
            if let Some(v) = self.get_bool(k) {
                out.push(format!("{k} = {v}"));
            }
        }
        out
    }
}

/// O arquivo lido e validado.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub global: Layer,
    pub profiles: BTreeMap<String, Layer>,
    pub projects: BTreeMap<PathBuf, Layer>,
}

impl Config {
    /// Lê o arquivo. Um arquivo que não existe é uma configuração vazia.
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Config::parse(path, &text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(ConfigError {
                path: path.into(),
                line: None,
                message: format!("não foi possível ler o arquivo: {e}"),
            }),
        }
    }

    /// Valida o texto de um arquivo (`path` só aparece nas mensagens).
    pub fn parse(path: &Path, text: &str) -> Result<Config, ConfigError> {
        let err = |span: Option<std::ops::Range<usize>>, message: String| ConfigError {
            path: path.into(),
            line: span.map(|s| line_of(text, s.start)),
            message,
        };
        let doc = Document::parse(text)
            .map_err(|e| err(e.span(), e.message().trim().replace('\n', " ").to_string()))?;
        let mut cfg = Config::default();
        for (key, item) in doc.as_table().iter() {
            let kspan = doc.as_table().key(key).and_then(|k| k.span());
            match key {
                "global" => {
                    let t = item.as_table_like().ok_or_else(|| {
                        err(
                            kspan.clone(),
                            "`global` precisa ser uma seção `[global]`".into(),
                        )
                    })?;
                    cfg.global = layer(t, &err)?;
                }
                "profile" | "project" => {
                    let t = item.as_table_like().ok_or_else(|| {
                        err(
                            kspan.clone(),
                            format!("`{key}` precisa ser uma seção `[{key}.<nome>]`"),
                        )
                    })?;
                    for (name, sub) in t.iter() {
                        let nspan = t.key(name).and_then(|k| k.span()).or(kspan.clone());
                        let st = sub.as_table_like().ok_or_else(|| {
                            err(
                                nspan.clone(),
                                format!("`{key}.{name}` precisa ser uma seção"),
                            )
                        })?;
                        let l = layer(st, &err)?;
                        if key == "profile" {
                            if !valid_profile_name(name) {
                                return Err(err(
                                    nspan,
                                    format!(
                                        "nome de perfil inválido: {name:?}. Use letras minúsculas, números, `-` e `_`, começando por letra ou número"
                                    ),
                                ));
                            }
                            cfg.profiles.insert(name.to_string(), l);
                        } else {
                            let p = PathBuf::from(name);
                            if !p.is_absolute() {
                                return Err(err(
                                    nspan,
                                    format!(
                                        "a pasta de um projeto precisa ser um caminho absoluto: {name:?}"
                                    ),
                                ));
                            }
                            cfg.projects.insert(normalize(&p), l);
                        }
                    }
                }
                other => {
                    return Err(err(
                        kspan,
                        format!(
                            "seção desconhecida: `{other}`. Seções aceitas: `[global]`, `[profile.<nome>]` e `[project.\"<pasta>\"]`"
                        ),
                    ));
                }
            }
        }
        Ok(cfg)
    }

    /// Nomes dos perfis, para mensagens.
    pub fn profile_names(&self) -> String {
        if self.profiles.is_empty() {
            "nenhum perfil definido".into()
        } else {
            self.profiles.keys().cloned().collect::<Vec<_>>().join(", ")
        }
    }
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

type ErrFn<'a> = dyn Fn(Option<std::ops::Range<usize>>, String) -> ConfigError + 'a;

fn layer(t: &dyn toml_edit::TableLike, err: &ErrFn<'_>) -> Result<Layer, ConfigError> {
    let mut l = Layer::default();
    for (key, item) in t.iter() {
        let kspan = t.key(key).and_then(|k| k.span());
        let vspan = item.span().or(kspan.clone());
        match key {
            "ro" | "rw" => {
                let items = strings(item, key, vspan.clone(), err)?;
                let mut out = Vec::new();
                for (s, span) in items {
                    if !valid_path(&s) {
                        return Err(err(
                            span,
                            format!(
                                "`{key}` aceita só caminhos absolutos ou iniciados por `~/`: {s:?}"
                            ),
                        ));
                    }
                    out.push(PathBuf::from(s));
                }
                if key == "ro" {
                    l.ro = out;
                } else {
                    l.rw = out;
                }
            }
            "pass-env" => {
                for (s, span) in strings(item, key, vspan.clone(), err)? {
                    if !valid_var_name(&s) {
                        return Err(err(
                            span,
                            format!(
                                "`pass-env` aceita só nomes de variáveis, nunca valores: {s:?}"
                            ),
                        ));
                    }
                    l.pass_env.push(s);
                }
            }
            "repo" | "kvm" | "gpu" | "clipboard" => {
                let v = item.as_bool().ok_or_else(|| {
                    err(vspan.clone(), format!("`{key}` precisa ser true ou false"))
                })?;
                *l.bool_mut(key).expect("chave de liga e desliga") = Some(v);
            }
            other => {
                return Err(err(
                    kspan,
                    format!(
                        "chave desconhecida: `{other}`. Chaves aceitas: {}",
                        LIST_KEYS
                            .iter()
                            .chain(BOOL_KEYS)
                            .map(|k| format!("`{k}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
        }
    }
    let rw: Vec<PathBuf> = l.rw.iter().map(|p| normalize(p)).collect();
    if let Some(p) = l.ro.iter().find(|p| rw.contains(&normalize(p))) {
        let span = t.key("rw").and_then(|k| k.span());
        return Err(err(
            span,
            format!("{} aparece em `ro` e em `rw` na mesma seção", p.display()),
        ));
    }
    Ok(l)
}

type Spanned = (String, Option<std::ops::Range<usize>>);

fn strings(
    item: &Item,
    key: &str,
    span: Option<std::ops::Range<usize>>,
    err: &ErrFn<'_>,
) -> Result<Vec<Spanned>, ConfigError> {
    let arr = item.as_array().ok_or_else(|| {
        err(
            span.clone(),
            format!("`{key}` precisa ser uma lista de textos"),
        )
    })?;
    arr.iter()
        .map(|v: &Value| {
            v.as_str()
                .map(|s| (s.to_string(), v.span().or(span.clone())))
                .ok_or_else(|| {
                    err(
                        v.span().or(span.clone()),
                        format!("`{key}` precisa ser uma lista de textos"),
                    )
                })
        })
        .collect()
}

/// Caminho aceito no arquivo: absoluto ou iniciado por `~/`.
pub fn valid_path(s: &str) -> bool {
    (s.starts_with('/') || (s.starts_with("~/") && s.len() > 2)) && !s.contains('\0')
}

/// Nome de variável de ambiente: `[A-Za-z_][A-Za-z0-9_]*`.
pub fn valid_var_name(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Nome de perfil: `[a-z0-9][a-z0-9_-]*`.
pub fn valid_profile_name(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_lowercase() || ch.is_ascii_digit())
        && c.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
}

/// Normalização léxica: tira `.`, resolve `..` sem tocar no disco e a barra
/// final.
pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Expande `~/` com o HOME e normaliza.
pub fn expand(p: &Path, home: &Path) -> PathBuf {
    match p.strip_prefix("~") {
        Ok(rest) => normalize(&home.join(rest)),
        Err(_) => normalize(p),
    }
}

/// De onde veio um valor da política efetiva.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Baseline ou preset.
    Default,
    Global,
    Profile(String),
    Project(PathBuf),
    Run,
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Origin::Default => write!(f, "padrão"),
            Origin::Global => write!(f, "modificada (global)"),
            Origin::Profile(n) => write!(f, "modificada (perfil {n})"),
            Origin::Project(p) => write!(f, "modificada (projeto {})", p.display()),
            Origin::Run => write!(f, "modificada (opção do run)"),
        }
    }
}

/// Um valor de liga e desliga com a origem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    pub value: bool,
    pub origin: Origin,
}

/// Uma pasta extra da política efetiva.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraPath {
    pub path: PathBuf,
    pub writable: bool,
    pub origin: Origin,
}

/// O que o lançamento pede.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    pub harness: &'a str,
    /// Pasta de trabalho, absoluta.
    pub workdir: &'a Path,
    pub profile: Option<&'a str>,
    /// Opções dadas no `run` (caminhos já absolutos).
    pub run: &'a Layer,
}

/// A política resolvida, com a origem de cada valor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    pub extras: Vec<ExtraPath>,
    pub pass_env: Vec<(String, Origin)>,
    pub repo: Setting,
    pub kvm: Setting,
    pub gpu: Setting,
    pub clipboard: Setting,
    pub profile: Option<String>,
    /// Pasta do perfil de projeto aplicado.
    pub project: Option<PathBuf>,
}

impl Effective {
    pub fn ro(&self) -> Vec<PathBuf> {
        self.extras
            .iter()
            .filter(|e| !e.writable)
            .map(|e| e.path.clone())
            .collect()
    }

    pub fn rw(&self) -> Vec<PathBuf> {
        self.extras
            .iter()
            .filter(|e| e.writable)
            .map(|e| e.path.clone())
            .collect()
    }

    pub fn pass_env_names(&self) -> Vec<String> {
        self.pass_env.iter().map(|(n, _)| n.clone()).collect()
    }

    /// As opções de composição desta política.
    pub fn run_options(
        &self,
        harness: &str,
        workdir: &Path,
        args: &[String],
        layer: LayerMode,
    ) -> RunOptions {
        RunOptions {
            harness: harness.into(),
            workdir: Some(workdir.into()),
            ro: self.ro(),
            rw: self.rw(),
            pass_env: self.pass_env_names(),
            repo: self.repo.value,
            kvm: self.kvm.value,
            no_gpu: !self.gpu.value,
            args: args.to_vec(),
            layer,
        }
    }

    pub fn setting(&self, key: &str) -> Option<&Setting> {
        match key {
            "repo" => Some(&self.repo),
            "kvm" => Some(&self.kvm),
            "gpu" => Some(&self.gpu),
            "clipboard" => Some(&self.clipboard),
            _ => None,
        }
    }
}

/// Erros da resolução.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    UnknownProfile { name: String, available: String },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::UnknownProfile { name, available } => write!(
                f,
                "o perfil {name:?} não existe no arquivo de configuração. Perfis: {available}"
            ),
        }
    }
}

impl std::error::Error for ResolveError {}

/// Canoniza quando o caminho existe; senão, só normaliza.
fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| normalize(p))
}

/// O perfil de projeto que vale para `workdir`: o da pasta mais próxima.
pub fn project_for<'c>(cfg: &'c Config, workdir: &Path) -> Option<(&'c PathBuf, &'c Layer)> {
    let wd = canon(workdir);
    cfg.projects
        .iter()
        .filter(|(p, _)| wd.starts_with(canon(p)))
        .max_by_key(|(p, _)| canon(p).components().count())
}

/// Combina as camadas: baseline e preset (sem chaves hoje), global, perfil
/// nomeado, projeto e `run`.
pub fn resolve(cfg: &Config, host: &Host, req: &Request<'_>) -> Result<Effective, ResolveError> {
    let mut layers: Vec<(&Layer, Origin)> = vec![(&cfg.global, Origin::Global)];
    if let Some(name) = req.profile {
        let l = cfg
            .profiles
            .get(name)
            .ok_or_else(|| ResolveError::UnknownProfile {
                name: name.into(),
                available: cfg.profile_names(),
            })?;
        layers.push((l, Origin::Profile(name.into())));
    }
    let project = project_for(cfg, req.workdir);
    if let Some((p, l)) = project {
        layers.push((l, Origin::Project(p.clone())));
    }
    layers.push((req.run, Origin::Run));

    let default = |value| Setting {
        value,
        origin: Origin::Default,
    };
    let mut eff = Effective {
        extras: Vec::new(),
        pass_env: Vec::new(),
        repo: default(false),
        kvm: default(false),
        gpu: default(true),
        clipboard: default(false),
        profile: req.profile.map(String::from),
        project: project.map(|(p, _)| p.clone()),
    };
    for (l, origin) in layers {
        for key in BOOL_KEYS {
            if let Some(v) = l.get_bool(key) {
                let s = match *key {
                    "repo" => &mut eff.repo,
                    "kvm" => &mut eff.kvm,
                    "gpu" => &mut eff.gpu,
                    _ => &mut eff.clipboard,
                };
                *s = Setting {
                    value: v,
                    origin: origin.clone(),
                };
            }
        }
        for (list, writable) in [(&l.ro, false), (&l.rw, true)] {
            for p in list {
                let path = expand(p, &host.home);
                // A camada mais específica decide; dentro de uma camada, `rw`
                // vem depois de `ro`.
                eff.extras.retain(|e| e.path != path);
                eff.extras.push(ExtraPath {
                    path,
                    writable,
                    origin: origin.clone(),
                });
            }
        }
        for n in &l.pass_env {
            if !eff.pass_env.iter().any(|(e, _)| e == n) {
                eff.pass_env.push((n.clone(), origin.clone()));
            }
        }
    }
    let _ = req.harness;
    Ok(eff)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(home: &Path) -> Host {
        Host {
            home: home.into(),
            uid: 1000,
            gid: 1000,
            user: "ana".into(),
            group: "ana".into(),
            user_shell: "/bin/bash".into(),
            data_dir: home.join(".local/share/sandmux"),
            config_dir: home.join(".config/sandmux"),
            cwd: home.into(),
            env: Default::default(),
            sys_root: "/".into(),
        }
    }

    fn parse(text: &str) -> Result<Config, ConfigError> {
        Config::parse(Path::new("/c/config.toml"), text)
    }

    fn line(text: &str) -> usize {
        parse(text).unwrap_err().line.expect("linha")
    }

    #[test]
    fn as_tres_secoes_e_as_chaves() {
        let c = parse(
            r#"
# comentário
[global]
ro = ["~/docs", "/opt/ref"]
gpu = false

[profile.web]
clipboard = true
pass-env = ["OPENROUTER_API_KEY"]

[project."/home/ana/proj"]
rw = ["~/saida"]
kvm = true
repo = true
"#,
        )
        .unwrap();
        assert_eq!(
            c.global.ro,
            vec![PathBuf::from("~/docs"), "/opt/ref".into()]
        );
        assert_eq!(c.global.gpu, Some(false));
        assert_eq!(c.profiles["web"].clipboard, Some(true));
        assert_eq!(c.profiles["web"].pass_env, vec!["OPENROUTER_API_KEY"]);
        let p = &c.projects[Path::new("/home/ana/proj")];
        assert_eq!(p.kvm, Some(true));
        assert_eq!(p.repo, Some(true));
        assert_eq!(p.rw, vec![PathBuf::from("~/saida")]);
    }

    #[test]
    fn arquivo_ausente_e_vazio() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            Config::load(&d.path().join("nada.toml")).unwrap(),
            Config::default()
        );
        assert_eq!(parse("").unwrap(), Config::default());
    }

    #[test]
    fn toml_malformado_com_linha() {
        let e = parse("[global]\nro = []\n\n[profile.x\nkvm = true\n").unwrap_err();
        assert_eq!(e.line, Some(4), "{e}");
        assert!(e.to_string().contains("/c/config.toml"), "{e}");
    }

    #[test]
    fn secao_desconhecida() {
        let t = "[global]\n\n[rede]\nmodo = 1\n";
        assert_eq!(line(t), 3);
        assert!(parse(t).unwrap_err().message.contains("rede"));
        assert!(parse("solto = true\n").is_err());
    }

    #[test]
    fn chave_desconhecida() {
        let t = "[global]\nkvm = true\nrede = \"aberta\"\n";
        let e = parse(t).unwrap_err();
        assert_eq!(e.line, Some(3));
        assert!(e.message.contains("`rede`"), "{e}");
    }

    #[test]
    fn tipo_errado() {
        let t = "[global]\n\nkvm = \"sim\"\n";
        let e = parse(t).unwrap_err();
        assert_eq!(e.line, Some(3));
        assert!(e.message.contains("`kvm` precisa ser true ou false"), "{e}");
        assert!(parse("[global]\nro = \"~/docs\"\n").is_err());
        assert!(parse("[global]\nro = [1]\n").is_err());
    }

    #[test]
    fn caminho_relativo() {
        let e = parse("[global]\nrw = [\n  \"/ok\",\n  \"saida\",\n]\n").unwrap_err();
        assert_eq!(e.line, Some(4), "{e}");
        assert!(parse("[global]\nro = [\"~\"]\n").is_err());
        assert!(parse("[global]\nro = [\"~ana/x\"]\n").is_err());
    }

    #[test]
    fn projeto_com_caminho_relativo() {
        assert_eq!(line("[global]\n[project.\"proj\"]\nkvm = true\n"), 2);
    }

    #[test]
    fn nome_de_variavel() {
        let e = parse("[global]\npass-env = [\"OPENROUTER_API_KEY=sk-123\"]\n").unwrap_err();
        assert_eq!(e.line, Some(2));
        assert!(e.message.contains("nunca valores"), "{e}");
        // O valor nunca aparece inteiro? Aparece o item recusado, que é o que
        // o usuário escreveu: a mensagem precisa apontar qual é.
        assert!(parse("[global]\npass-env = [\"1X\"]\n").is_err());
        assert!(parse("[global]\npass-env = [\"_X1\"]\n").is_ok());
    }

    #[test]
    fn nome_de_perfil() {
        assert_eq!(line("[global]\n\n[profile.Web]\n"), 3);
        assert!(parse("[profile.web-2_x]\n").is_ok());
    }

    #[test]
    fn mesmo_caminho_em_ro_e_rw() {
        let e = parse("[global]\nro = [\"~/docs\"]\nrw = [\"~/docs/\"]\n").unwrap_err();
        assert_eq!(e.line, Some(3));
        assert!(e.message.contains("ro") && e.message.contains("rw"), "{e}");
    }

    fn req<'a>(workdir: &'a Path, profile: Option<&'a str>, run: &'a Layer) -> Request<'a> {
        Request {
            harness: "shell",
            workdir,
            profile,
            run,
        }
    }

    #[test]
    fn camadas_e_padroes() {
        let home = Path::new("/home/ana");
        let h = host(home);
        let run = Layer::default();
        let e = resolve(&Config::default(), &h, &req(Path::new("/w"), None, &run)).unwrap();
        assert!(e.gpu.value && !e.kvm.value && !e.clipboard.value && !e.repo.value);
        assert_eq!(e.gpu.origin, Origin::Default);
        assert!(e.extras.is_empty());

        let c = parse("[global]\ngpu = false\nkvm = true\n").unwrap();
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &run)).unwrap();
        assert!(!e.gpu.value);
        assert_eq!(e.gpu.origin, Origin::Global);
        assert!(e.kvm.value);
    }

    #[test]
    fn precedencia_liga_desliga() {
        let h = host(Path::new("/home/ana"));
        let c = parse(
            "[global]\ngpu = false\n[profile.vm]\nkvm = true\n[project.\"/w\"]\ngpu = true\nkvm = true\n",
        )
        .unwrap();
        let none = Layer::default();
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &none)).unwrap();
        assert!(e.gpu.value);
        assert_eq!(e.gpu.origin, Origin::Project("/w".into()));
        // O run vence o projeto.
        let run = Layer {
            kvm: Some(false),
            ..Default::default()
        };
        let e = resolve(&c, &h, &req(Path::new("/w"), Some("vm"), &run)).unwrap();
        assert!(!e.kvm.value);
        assert_eq!(e.kvm.origin, Origin::Run);
        // O projeto vence o perfil nomeado.
        let c2 = parse("[profile.vm]\nkvm = true\n[project.\"/w\"]\nkvm = false\n").unwrap();
        let e = resolve(&c2, &h, &req(Path::new("/w"), Some("vm"), &none)).unwrap();
        assert!(!e.kvm.value);
    }

    #[test]
    fn listas_acumuladas() {
        let h = host(Path::new("/home/ana"));
        let c = parse(
            "[global]\nro = [\"~/docs\"]\nrw = [\"~/saida\"]\npass-env = [\"A\"]\n[project.\"/w\"]\nro = [\"~/ref\", \"/home/ana/saida\"]\npass-env = [\"A\", \"B\"]\n",
        )
        .unwrap();
        let none = Layer::default();
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &none)).unwrap();
        assert_eq!(
            e.ro(),
            vec![
                PathBuf::from("/home/ana/docs"),
                "/home/ana/ref".into(),
                "/home/ana/saida".into()
            ]
        );
        assert!(e.rw().is_empty());
        assert_eq!(e.pass_env_names(), vec!["A", "B"]);
        assert_eq!(e.pass_env[1].1, Origin::Project("/w".into()));
        // Fora do projeto, `saida` continua com escrita.
        let e = resolve(&c, &h, &req(Path::new("/x"), None, &none)).unwrap();
        assert_eq!(e.rw(), vec![PathBuf::from("/home/ana/saida")]);
    }

    #[test]
    fn perfil_de_projeto_pela_pasta() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        for p in ["proj/api/src", "proj2"] {
            std::fs::create_dir_all(base.join(p)).unwrap();
        }
        std::os::unix::fs::symlink(base.join("proj"), base.join("atalho")).unwrap();
        let text = format!(
            "[project.\"{b}/proj\"]\nkvm = true\n[project.\"{b}/proj/api\"]\nclipboard = true\n",
            b = base.display()
        );
        let c = parse(&text).unwrap();
        let h = host(Path::new("/home/ana"));
        let none = Layer::default();
        let at = |w: &str| resolve(&c, &h, &req(&base.join(w), None, &none)).unwrap();
        // Subpasta.
        let e = at("proj/api/../..//proj");
        assert_eq!(e.project, Some(base.join("proj")));
        // Aninhados: só o mais próximo.
        let e = at("proj/api/src");
        assert_eq!(e.project, Some(base.join("proj/api")));
        assert!(e.clipboard.value && !e.kvm.value);
        // /proj não casa com /proj2.
        assert_eq!(at("proj2").project, None);
        // Link para a pasta.
        assert_eq!(at("atalho").project, Some(base.join("proj")));
    }

    #[test]
    fn perfil_inexistente() {
        let c = parse("[profile.vm]\n[profile.web]\n").unwrap();
        let h = host(Path::new("/home/ana"));
        let none = Layer::default();
        let e = resolve(&c, &h, &req(Path::new("/w"), Some("nada"), &none)).unwrap_err();
        let s = e.to_string();
        assert!(s.contains("nada") && s.contains("vm, web"), "{s}");
    }

    /// HOME de teste com `docs` e a pasta de configuração.
    fn compose_with(text: &str, workdir: &str) -> Result<crate::Composed, crate::PolicyError> {
        let d = tempfile::tempdir().unwrap();
        let home = d.path().canonicalize().unwrap();
        for p in ["docs", ".config/sandmux", "proj", ".ssh"] {
            std::fs::create_dir_all(home.join(p)).unwrap();
        }
        std::fs::write(home.join(".config/sandmux/config.toml"), "").unwrap();
        let mut h = host(&home);
        h.cwd = home.join("proj");
        let c = parse(text).unwrap();
        let wd = home.join(workdir);
        let none = Layer::default();
        let e = resolve(&c, &h, &req(&wd, None, &none)).unwrap();
        let r = crate::compose(&h, &e.run_options("shell", &wd, &[], LayerMode::Temporary));
        drop(d);
        r
    }

    #[test]
    fn caminhos_da_configuracao_passam_pela_composicao() {
        let c = compose_with("[global]\nro = [\"~/docs\"]\n", "proj").unwrap();
        let ro = c.spec.mounts.iter().any(|m| {
            matches!(m,
            sandmux_spec::Mount::Bind { source, writable: false, .. } if source.ends_with("docs"))
        });
        assert!(ro, "{:?}", c.spec.mounts);
        assert!(matches!(
            compose_with("[global]\nro = [\"~/nada\"]\n", "proj"),
            Err(crate::PolicyError::MissingPath(_))
        ));
        // Aviso de credencial também para caminho vindo da configuração.
        let c = compose_with("[global]\nro = [\"~/.ssh\"]\n", "proj").unwrap();
        assert!(
            c.warnings.iter().any(|w| w.contains("credenciais reais")),
            "{:?}",
            c.warnings
        );
    }

    #[test]
    fn pasta_de_configuracao_sem_escrita() {
        let e = compose_with("[global]\nrw = [\"~/.config\"]\n", "proj").unwrap_err();
        assert!(e.to_string().contains("configuração do sandmux"), "{e}");
        assert!(compose_with("[global]\nro = [\"~/.config\"]\n", "proj").is_ok());
        let e = compose_with("", ".config/sandmux").unwrap_err();
        assert!(e.to_string().contains("configuração do sandmux"), "{e}");
        let e = compose_with(
            "[global]\nrw = [\"~/.config/sandmux/config.toml\"]\n",
            "proj",
        )
        .unwrap_err();
        assert!(e.to_string().contains("configuração do sandmux"), "{e}");
    }

    #[test]
    fn origem_mostrada() {
        assert_eq!(Origin::Default.to_string(), "padrão");
        assert_eq!(
            Origin::Project("/home/ana/proj".into()).to_string(),
            "modificada (projeto /home/ana/proj)"
        );
    }
}
