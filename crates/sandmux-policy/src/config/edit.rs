//! Edição do arquivo de configuração preservando o resto do texto:
//! comentários, ordem e formatação do que não foi tocado.

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use toml_edit::{Array, DocumentMut, Item, Table, value};

use super::{
    BOOL_KEYS, Config, ConfigError, LIST_KEYS, expand, valid_path, valid_profile_name,
    valid_var_name,
};
use crate::host::Host;

/// Onde a mudança é feita.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Global,
    Profile(String),
    /// Pasta do projeto, absoluta.
    Project(PathBuf),
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::Global => write!(f, "[global]"),
            Target::Profile(n) => write!(f, "[profile.{n}]"),
            Target::Project(p) => write!(f, "[project.\"{}\"]", p.display()),
        }
    }
}

/// A mudança pedida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Set(String, bool),
    Unset(String),
    Add(String, String),
    Remove(String, String),
}

/// O que aconteceu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// O arquivo foi gravado.
    pub changed: bool,
    /// Explicação quando nada mudou.
    pub notice: Option<String>,
}

/// Erros de edição.
#[derive(Debug)]
pub enum EditError {
    /// O arquivo atual já é inválido.
    Invalid(ConfigError),
    /// A mudança foi recusada.
    Refused(String),
    Io(String),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::Invalid(e) => write!(f, "{e}"),
            EditError::Refused(m) | EditError::Io(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for EditError {}

/// Interpreta um valor de liga e desliga da linha de comando.
pub fn parse_bool(s: &str) -> Option<bool> {
    match s.to_lowercase().as_str() {
        "true" | "sim" => Some(true),
        "false" | "não" | "nao" => Some(false),
        _ => None,
    }
}

fn refused(m: impl Into<String>) -> EditError {
    EditError::Refused(m.into())
}

/// Confere a chave e o item antes de tocar no arquivo.
fn check_op(op: &Op, host: &Host) -> Result<(), EditError> {
    match op {
        Op::Set(k, _) | Op::Unset(k) => {
            if !BOOL_KEYS.contains(&k.as_str()) {
                let hint = if LIST_KEYS.contains(&k.as_str()) {
                    format!("; `{k}` é uma lista, use `add` e `remove`")
                } else {
                    String::new()
                };
                return Err(refused(format!(
                    "chave desconhecida para `set` e `unset`: `{k}`. Chaves aceitas: {}{hint}",
                    BOOL_KEYS.join(", ")
                )));
            }
        }
        Op::Add(k, item) | Op::Remove(k, item) => {
            if !LIST_KEYS.contains(&k.as_str()) {
                let hint = if BOOL_KEYS.contains(&k.as_str()) {
                    format!("; `{k}` é liga e desliga, use `set` e `unset`")
                } else {
                    String::new()
                };
                return Err(refused(format!(
                    "chave desconhecida para `add` e `remove`: `{k}`. Chaves aceitas: {}{hint}",
                    LIST_KEYS.join(", ")
                )));
            }
            if k == "pass-env" {
                if !valid_var_name(item) {
                    return Err(refused(format!(
                        "`pass-env` aceita só nomes de variáveis, nunca valores: {item:?}"
                    )));
                }
            } else if !valid_path(item) {
                return Err(refused(format!(
                    "`{k}` aceita só caminhos absolutos ou iniciados por `~/`: {item:?}"
                )));
            }
            if k == "rw" && matches!(op, Op::Add(..)) {
                let p = expand(Path::new(item), &host.home);
                let p = p.canonicalize().unwrap_or(p);
                crate::check_dangerous(host, &p, true).map_err(|e| refused(e.to_string()))?;
            }
        }
    }
    Ok(())
}

/// A tabela do alvo, criada quando falta.
fn target_table<'d>(doc: &'d mut DocumentMut, target: &Target) -> Result<&'d mut Table, EditError> {
    let (parent, key) = match target {
        Target::Global => {
            let item = doc.entry("global").or_insert(Item::Table(Table::new()));
            return item
                .as_table_mut()
                .ok_or_else(|| refused("`global` no arquivo não é uma seção"));
        }
        Target::Profile(n) => ("profile", n.clone()),
        Target::Project(p) => ("project", p.display().to_string()),
    };
    let item = doc.entry(parent).or_insert_with(|| {
        let mut t = Table::new();
        t.set_implicit(true);
        Item::Table(t)
    });
    let parent_t = item
        .as_table_mut()
        .ok_or_else(|| refused(format!("`{parent}` no arquivo não é uma seção")))?;
    parent_t
        .entry(&key)
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| refused(format!("`{parent}.{key}` no arquivo não é uma seção")))
}

/// Tira a seção do alvo quando ficou vazia, e o pai implícito também.
fn prune(doc: &mut DocumentMut, target: &Target) {
    let (parent, key) = match target {
        Target::Global => {
            if doc
                .get("global")
                .and_then(Item::as_table)
                .is_some_and(Table::is_empty)
            {
                doc.remove("global");
            }
            return;
        }
        Target::Profile(n) => ("profile", n.clone()),
        Target::Project(p) => ("project", p.display().to_string()),
    };
    let Some(pt) = doc.get_mut(parent).and_then(Item::as_table_mut) else {
        return;
    };
    if pt
        .get(&key)
        .and_then(Item::as_table)
        .is_some_and(Table::is_empty)
    {
        pt.remove(&key);
    }
    if pt.is_empty() {
        doc.remove(parent);
    }
}

/// Aplica `op` em `target` no arquivo `path`.
pub fn apply(path: &Path, host: &Host, target: &Target, op: &Op) -> Result<Outcome, EditError> {
    if let Target::Profile(n) = target
        && !valid_profile_name(n)
    {
        return Err(refused(format!(
            "nome de perfil inválido: {n:?}. Use letras minúsculas, números, `-` e `_`, começando por letra ou número"
        )));
    }
    if let Target::Project(p) = target
        && !p.is_absolute()
    {
        return Err(refused(format!(
            "a pasta de um projeto precisa ser um caminho absoluto: {}",
            p.display()
        )));
    }
    check_op(op, host)?;
    let original = match std::fs::read_to_string(path) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(EditError::Io(format!("ler {}: {e}", path.display()))),
    };
    let text = original.clone().unwrap_or_default();
    // O arquivo atual precisa ser válido: editar por cima de um erro
    // esconderia o erro.
    Config::parse(path, &text).map_err(EditError::Invalid)?;
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| EditError::Io(e.to_string()))?;

    let mut notice = None;
    {
        let t = target_table(&mut doc, target)?;
        match op {
            Op::Set(k, v) => {
                if t.get(k).and_then(Item::as_bool) == Some(*v) {
                    notice = Some(format!("`{k}` já é {v} em {target}"));
                } else if let Some(Item::Value(old)) = t.get_mut(k) {
                    // Mantém o comentário ao lado do valor.
                    let decor = old.decor().clone();
                    *old = (*v).into();
                    *old.decor_mut() = decor;
                } else {
                    t.insert(k, value(*v));
                }
            }
            Op::Unset(k) => {
                if t.remove(k).is_none() {
                    notice = Some(format!("`{k}` não estava definida em {target}"));
                }
            }
            Op::Add(k, item) => {
                let arr = t
                    .entry(k)
                    .or_insert(value(Array::new()))
                    .as_array_mut()
                    .ok_or_else(|| refused(format!("`{k}` no arquivo não é uma lista")))?;
                if arr.iter().any(|v| v.as_str() == Some(item)) {
                    notice = Some(format!("{item} já está em `{k}` de {target}"));
                } else {
                    arr.push(item.as_str());
                }
            }
            Op::Remove(k, item) => {
                let arr = t.get_mut(k).and_then(Item::as_array_mut);
                let Some(arr) = arr else {
                    return Err(refused(format!("{item} não está em `{k}` de {target}")));
                };
                let Some(i) = arr.iter().position(|v| v.as_str() == Some(item)) else {
                    return Err(refused(format!("{item} não está em `{k}` de {target}")));
                };
                arr.remove(i);
                if arr.is_empty() {
                    t.remove(k);
                }
            }
        }
    }
    prune(&mut doc, target);
    let mut new_text = doc.to_string();
    // Uma seção apagada no começo deixa a linha em branco da seguinte no topo.
    if !text.starts_with('\n') {
        new_text = new_text.trim_start_matches('\n').to_string();
    }
    if notice.is_some() || original.as_deref() == Some(new_text.as_str()) {
        return Ok(Outcome {
            changed: false,
            notice,
        });
    }
    if original.is_none() && new_text.trim().is_empty() {
        return Ok(Outcome {
            changed: false,
            notice: Some(format!("nada a remover em {target}")),
        });
    }
    // O resultado precisa ser válido antes de ir para o disco.
    let cfg = Config::parse(path, &new_text).map_err(|e| refused(e.to_string()))?;
    let _ = cfg;
    write_atomic(path, &new_text)
        .map_err(|e| EditError::Io(format!("gravar {}: {e}", path.display())))?;
    Ok(Outcome {
        changed: true,
        notice: None,
    })
}

/// Grava num arquivo temporário da mesma pasta e troca. Arquivo novo com 600
/// e pasta nova com 700; um arquivo existente mantém o modo.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let mode = std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o7777)
        .unwrap_or(0o600);
    let tmp = dir.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config.toml"),
        std::process::id()
    ));
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))?;
    std::fs::rename(&tmp, path)
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

    struct T {
        _d: tempfile::TempDir,
        h: Host,
        f: PathBuf,
    }

    fn setup() -> T {
        let d = tempfile::tempdir().unwrap();
        let home = d.path().canonicalize().unwrap();
        let h = host(&home);
        let f = h.config_dir.join("config.toml");
        T { _d: d, h, f }
    }

    fn run(t: &T, target: Target, op: Op) -> Result<Outcome, EditError> {
        apply(&t.f, &t.h, &target, &op)
    }

    fn read(t: &T) -> String {
        std::fs::read_to_string(&t.f).unwrap()
    }

    #[test]
    fn set_cria_o_perfil() {
        let t = setup();
        run(
            &t,
            Target::Profile("vm".into()),
            Op::Set("kvm".into(), true),
        )
        .unwrap();
        assert_eq!(read(&t), "[profile.vm]\nkvm = true\n");
        use std::os::unix::fs::MetadataExt;
        assert_eq!(std::fs::metadata(&t.f).unwrap().mode() & 0o777, 0o600);
        assert_eq!(
            std::fs::metadata(t.f.parent().unwrap()).unwrap().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn so_a_diferenca_no_arquivo_novo() {
        let t = setup();
        run(&t, Target::Global, Op::Add("ro".into(), "~/docs".into())).unwrap();
        assert_eq!(read(&t), "[global]\nro = [\"~/docs\"]\n");
    }

    #[test]
    fn projeto_com_aspas() {
        let t = setup();
        run(
            &t,
            Target::Project("/home/ana/proj".into()),
            Op::Add("ro".into(), "~/docs".into()),
        )
        .unwrap();
        assert_eq!(
            read(&t),
            "[project.\"/home/ana/proj\"]\nro = [\"~/docs\"]\n"
        );
        let c = Config::load(&t.f).unwrap();
        assert!(c.projects.contains_key(Path::new("/home/ana/proj")));
    }

    #[test]
    fn comentario_preservado() {
        let t = setup();
        let original = "# Minha configuração\n[global]\n# GPU fica\ngpu = true # comentário ao lado\nro = [\n  \"~/docs\", # referência\n]\n\n[profile.vm]\nkvm = true\n";
        std::fs::create_dir_all(t.f.parent().unwrap()).unwrap();
        std::fs::write(&t.f, original).unwrap();
        run(&t, Target::Global, Op::Set("gpu".into(), false)).unwrap();
        let s = read(&t);
        assert_eq!(s, original.replace("gpu = true #", "gpu = false #"), "{s}");
    }

    #[test]
    fn unset_e_remove_limpam_secoes_vazias() {
        let t = setup();
        run(
            &t,
            Target::Profile("vm".into()),
            Op::Set("kvm".into(), true),
        )
        .unwrap();
        run(&t, Target::Global, Op::Add("ro".into(), "~/a".into())).unwrap();
        run(&t, Target::Profile("vm".into()), Op::Unset("kvm".into())).unwrap();
        assert_eq!(read(&t), "[global]\nro = [\"~/a\"]\n");
        run(&t, Target::Global, Op::Remove("ro".into(), "~/a".into())).unwrap();
        assert_eq!(read(&t), "");
    }

    #[test]
    fn repetido_ausente_e_ja_definido() {
        let t = setup();
        run(&t, Target::Global, Op::Add("ro".into(), "~/a".into())).unwrap();
        let o = run(&t, Target::Global, Op::Add("ro".into(), "~/a".into())).unwrap();
        assert!(!o.changed && o.notice.is_some());
        assert!(run(&t, Target::Global, Op::Remove("ro".into(), "~/b".into())).is_err());
        assert!(run(&t, Target::Global, Op::Remove("rw".into(), "~/b".into())).is_err());
        let o = run(&t, Target::Global, Op::Unset("kvm".into())).unwrap();
        assert!(!o.changed);
    }

    #[test]
    fn recusas_sem_tocar_no_arquivo() {
        let t = setup();
        run(&t, Target::Global, Op::Set("kvm".into(), true)).unwrap();
        let before = read(&t);
        for (target, op) in [
            (
                Target::Global,
                Op::Add("rw".into(), t.h.home.join(".config").display().to_string()),
            ),
            (
                Target::Global,
                Op::Add("rw".into(), "~/.config/sandmux".into()),
            ),
            (Target::Global, Op::Add("ro".into(), "relativo".into())),
            (Target::Global, Op::Add("pass-env".into(), "K=v".into())),
            (Target::Global, Op::Set("rede".into(), true)),
            (Target::Global, Op::Set("ro".into(), true)),
            (Target::Global, Op::Add("kvm".into(), "x".into())),
            (Target::Profile("Web".into()), Op::Set("kvm".into(), true)),
            (Target::Project("rel".into()), Op::Set("kvm".into(), true)),
        ] {
            assert!(
                run(&t, target.clone(), op.clone()).is_err(),
                "{target} {op:?}"
            );
        }
        // `ro` em ~/.config é aceito.
        run(&t, Target::Global, Op::Add("ro".into(), "~/.config".into())).unwrap();
        assert_ne!(read(&t), before);
    }

    #[test]
    fn mesmo_caminho_em_ro_e_rw_recusado() {
        let t = setup();
        run(&t, Target::Global, Op::Add("ro".into(), "~/docs".into())).unwrap();
        let e = run(&t, Target::Global, Op::Add("rw".into(), "~/docs".into())).unwrap_err();
        assert!(e.to_string().contains("mesma seção"), "{e}");
        assert_eq!(read(&t), "[global]\nro = [\"~/docs\"]\n");
    }

    #[test]
    fn arquivo_invalido_nao_e_editado() {
        let t = setup();
        std::fs::create_dir_all(t.f.parent().unwrap()).unwrap();
        std::fs::write(&t.f, "[global]\nrede = 1\n").unwrap();
        let e = run(&t, Target::Global, Op::Set("kvm".into(), true)).unwrap_err();
        assert!(matches!(e, EditError::Invalid(_)), "{e}");
        assert_eq!(read(&t), "[global]\nrede = 1\n");
    }

    #[test]
    fn valores_em_portugues() {
        assert_eq!(parse_bool("sim"), Some(true));
        assert_eq!(parse_bool("Não"), Some(false));
        assert_eq!(parse_bool("nao"), Some(false));
        assert_eq!(parse_bool("talvez"), None);
    }
}
