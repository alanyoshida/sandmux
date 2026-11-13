//! Especificação de lançamento do sandmux.
//!
//! É o contrato entre quem compõe a política (o cliente `sandmux` e, depois, o
//! daemon) e o lançador `sandmux-run`. O lançador não conhece harnesses nem
//! política: monta exatamente o que a especificação descreve e executa o
//! comando. O formato é JSON, com um campo de versão; um campo desconhecido ou
//! uma versão diferente fazem a leitura falhar.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Versão do formato que este código entende.
pub const SPEC_VERSION: u32 = 1;

/// Especificação completa de um lançamento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchSpec {
    /// Versão do formato. Precisa ser igual a [`SPEC_VERSION`].
    pub version: u32,
    /// Montagens, aplicadas em ordem sobre uma raiz nova e vazia.
    pub mounts: Vec<Mount>,
    /// Regras de Landlock aplicadas ao comando.
    pub landlock: LandlockRules,
    /// Filtro seccomp aplicado ao comando.
    pub seccomp: SeccompRules,
    /// Ambiente completo do comando. Nada é herdado de quem lança.
    pub env: Vec<EnvVar>,
    /// Comando e argumentos. O primeiro item é procurado no `PATH` de `env`
    /// quando não contém `/`.
    pub command: Vec<String>,
    /// Pasta em que o comando começa, dentro do sandbox.
    pub cwd: PathBuf,
}

/// Uma variável de ambiente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

/// Uma montagem na raiz nova. `dest` é sempre um caminho dentro do sandbox;
/// `source`, `lower` e os caminhos de camada são caminhos do host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mount {
    /// Sistema de arquivos em memória, novo e privado.
    Tmpfs { dest: PathBuf, mode: u32 },
    /// Pasta ou arquivo do host, recursivamente.
    Bind {
        source: PathBuf,
        dest: PathBuf,
        writable: bool,
    },
    /// Nó de dispositivo do host, com leitura e escrita.
    Device { source: PathBuf, dest: PathBuf },
    /// Pasta do host por baixo, em somente leitura, com uma camada de escrita
    /// por cima.
    Overlay {
        lower: PathBuf,
        dest: PathBuf,
        layer: Layer,
    },
    /// Cobre um arquivo ou pasta: o caminho passa a aparecer vazio e em
    /// somente leitura.
    Cover { dest: PathBuf },
    /// Arquivo gerado, em somente leitura.
    File {
        dest: PathBuf,
        contents: String,
        mode: u32,
    },
    /// Link simbólico criado na raiz nova.
    Symlink { dest: PathBuf, target: PathBuf },
    /// `/proc` novo, do pid namespace do sandbox.
    Proc { dest: PathBuf },
    /// Instância nova de pseudoterminais.
    Devpts { dest: PathBuf },
}

/// Onde fica a camada de escrita de um overlay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Layer {
    /// Camada no host, que persiste entre execuções.
    Persistent { upper: PathBuf, work: PathBuf },
    /// Camada em memória, descartada quando o sandbox termina.
    Temporary,
}

/// Regras de Landlock. Tudo que não está numa das listas fica inacessível.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandlockRules {
    /// Caminhos com leitura e execução.
    pub read: Vec<PathBuf>,
    /// Caminhos com todos os acessos.
    pub write: Vec<PathBuf>,
}

/// Filtro seccomp por lista de negação: as chamadas listadas devolvem erro de
/// permissão; as demais passam.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeccompRules {
    /// Nomes de chamadas de sistema negadas.
    pub deny_syscalls: Vec<String>,
    /// Nomes de comandos de `ioctl` negados.
    pub deny_ioctls: Vec<String>,
}

/// Erros de leitura e validação.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SpecError {
    #[error("especificação malformada: {0}")]
    Malformed(String),
    #[error(
        "versão de especificação não suportada: recebida {found}, suportada {supported}",
        supported = SPEC_VERSION
    )]
    UnsupportedVersion { found: String },
    #[error("especificação inválida: {0}")]
    Invalid(String),
}

impl LaunchSpec {
    /// Lê uma especificação em JSON, conferindo a versão antes dos campos, e
    /// valida o conteúdo.
    pub fn from_json(bytes: &[u8]) -> Result<Self, SpecError> {
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| SpecError::Malformed(e.to_string()))?;
        let found = match value.get("version") {
            Some(serde_json::Value::Number(n)) if n.as_u64() == Some(u64::from(SPEC_VERSION)) => {
                None
            }
            Some(other) => Some(other.to_string()),
            None => Some("ausente".to_string()),
        };
        if let Some(found) = found {
            return Err(SpecError::UnsupportedVersion { found });
        }
        let spec: LaunchSpec =
            serde_json::from_value(value).map_err(|e| SpecError::Malformed(e.to_string()))?;
        spec.validate()?;
        Ok(spec)
    }

    /// Serializa em JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a especificação sempre serializa")
    }

    /// Confere regras que o tipo não expressa: caminhos absolutos e sem `..`,
    /// comando não vazio, nomes de variáveis válidos.
    pub fn validate(&self) -> Result<(), SpecError> {
        if self.version != SPEC_VERSION {
            return Err(SpecError::UnsupportedVersion {
                found: self.version.to_string(),
            });
        }
        if self.command.is_empty() || self.command[0].is_empty() {
            return Err(SpecError::Invalid("comando vazio".into()));
        }
        check_path("cwd", &self.cwd)?;
        for m in &self.mounts {
            for (label, p) in m.paths() {
                check_path(label, p)?;
            }
            if let Mount::Symlink { target, .. } = m
                && target.as_os_str().is_empty()
            {
                return Err(SpecError::Invalid("link simbólico sem destino".into()));
            }
        }
        for p in self.landlock.read.iter().chain(&self.landlock.write) {
            check_path("landlock", p)?;
        }
        for v in &self.env {
            if v.name.is_empty() || v.name.contains('=') || v.name.contains('\0') {
                return Err(SpecError::Invalid(format!(
                    "nome de variável inválido: {:?}",
                    v.name
                )));
            }
            if v.value.contains('\0') {
                return Err(SpecError::Invalid(format!(
                    "valor com byte nulo na variável {}",
                    v.name
                )));
            }
        }
        Ok(())
    }

    /// Valor de uma variável do ambiente da especificação.
    pub fn env_value(&self, name: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|v| v.name == name)
            .map(|v| v.value.as_str())
    }
}

impl Mount {
    /// Destino da montagem dentro do sandbox.
    pub fn dest(&self) -> &Path {
        match self {
            Mount::Tmpfs { dest, .. }
            | Mount::Bind { dest, .. }
            | Mount::Device { dest, .. }
            | Mount::Overlay { dest, .. }
            | Mount::Cover { dest }
            | Mount::File { dest, .. }
            | Mount::Symlink { dest, .. }
            | Mount::Proc { dest }
            | Mount::Devpts { dest } => dest,
        }
    }

    fn paths(&self) -> Vec<(&'static str, &Path)> {
        let mut v: Vec<(&'static str, &Path)> = vec![("dest", self.dest())];
        match self {
            Mount::Bind { source, .. } | Mount::Device { source, .. } => {
                v.push(("source", source));
            }
            Mount::Overlay { lower, layer, .. } => {
                v.push(("lower", lower));
                if let Layer::Persistent { upper, work } = layer {
                    v.push(("upper", upper));
                    v.push(("work", work));
                }
            }
            _ => {}
        }
        v
    }
}

fn check_path(label: &str, p: &Path) -> Result<(), SpecError> {
    if !p.is_absolute() {
        return Err(SpecError::Invalid(format!(
            "{label}: caminho não absoluto: {}",
            p.display()
        )));
    }
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(SpecError::Invalid(format!(
            "{label}: caminho com '..': {}",
            p.display()
        )));
    }
    if p.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(SpecError::Invalid(format!(
            "{label}: caminho com byte nulo"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LaunchSpec {
        LaunchSpec {
            version: SPEC_VERSION,
            mounts: vec![
                Mount::Bind {
                    source: "/usr".into(),
                    dest: "/usr".into(),
                    writable: false,
                },
                Mount::Symlink {
                    dest: "/bin".into(),
                    target: "usr/bin".into(),
                },
                Mount::Proc {
                    dest: "/proc".into(),
                },
                Mount::Tmpfs {
                    dest: "/tmp".into(),
                    mode: 0o1777,
                },
                Mount::Overlay {
                    lower: "/home/u/.cargo".into(),
                    dest: "/home/u/.cargo".into(),
                    layer: Layer::Persistent {
                        upper: "/data/l/upper".into(),
                        work: "/data/l/work".into(),
                    },
                },
                Mount::Overlay {
                    lower: "/home/u/.npm".into(),
                    dest: "/home/u/.npm".into(),
                    layer: Layer::Temporary,
                },
                Mount::Cover {
                    dest: "/etc/shadow".into(),
                },
                Mount::File {
                    dest: "/etc/passwd".into(),
                    contents: "u:x:1000:1000::/home/u:/bin/bash\n".into(),
                    mode: 0o644,
                },
                Mount::Device {
                    source: "/dev/null".into(),
                    dest: "/dev/null".into(),
                },
                Mount::Devpts {
                    dest: "/dev/pts".into(),
                },
            ],
            landlock: LandlockRules {
                read: vec!["/".into()],
                write: vec!["/tmp".into()],
            },
            seccomp: SeccompRules {
                deny_syscalls: vec!["mount".into()],
                deny_ioctls: vec!["TIOCSTI".into()],
            },
            env: vec![EnvVar {
                name: "PATH".into(),
                value: "/usr/bin".into(),
            }],
            command: vec!["true".into()],
            cwd: "/tmp".into(),
        }
    }

    #[test]
    fn ida_e_volta() {
        let s = sample();
        let back = LaunchSpec::from_json(s.to_json().as_bytes()).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn recusa_campo_desconhecido_no_topo() {
        let mut v: serde_json::Value = serde_json::from_str(&sample().to_json()).unwrap();
        v["extra"] = serde_json::json!(true);
        let err = LaunchSpec::from_json(v.to_string().as_bytes()).unwrap_err();
        assert!(
            matches!(err, SpecError::Malformed(ref m) if m.contains("extra")),
            "{err}"
        );
    }

    #[test]
    fn recusa_campo_desconhecido_numa_montagem() {
        let mut v: serde_json::Value = serde_json::from_str(&sample().to_json()).unwrap();
        v["mounts"][0]["propagation"] = serde_json::json!("shared");
        let err = LaunchSpec::from_json(v.to_string().as_bytes()).unwrap_err();
        assert!(matches!(err, SpecError::Malformed(_)), "{err}");
    }

    #[test]
    fn recusa_tipo_de_montagem_desconhecido() {
        let mut v: serde_json::Value = serde_json::from_str(&sample().to_json()).unwrap();
        v["mounts"][0]["type"] = serde_json::json!("nfs");
        assert!(matches!(
            LaunchSpec::from_json(v.to_string().as_bytes()),
            Err(SpecError::Malformed(_))
        ));
    }

    #[test]
    fn recusa_versao_nao_suportada_antes_dos_campos() {
        let mut v: serde_json::Value = serde_json::from_str(&sample().to_json()).unwrap();
        v["version"] = serde_json::json!(2);
        v["campo_da_v2"] = serde_json::json!(1);
        let err = LaunchSpec::from_json(v.to_string().as_bytes()).unwrap_err();
        assert_eq!(err, SpecError::UnsupportedVersion { found: "2".into() });
        assert!(err.to_string().contains("recebida 2, suportada 1"));
    }

    #[test]
    fn recusa_versao_ausente() {
        let mut v: serde_json::Value = serde_json::from_str(&sample().to_json()).unwrap();
        v.as_object_mut().unwrap().remove("version");
        assert!(matches!(
            LaunchSpec::from_json(v.to_string().as_bytes()),
            Err(SpecError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn recusa_json_quebrado() {
        assert!(matches!(
            LaunchSpec::from_json(b"{\"version\": 1,"),
            Err(SpecError::Malformed(_))
        ));
    }

    #[test]
    fn recusa_caminho_relativo_ou_com_ponto_ponto() {
        let mut s = sample();
        s.cwd = "tmp".into();
        assert!(matches!(s.validate(), Err(SpecError::Invalid(_))));
        let mut s = sample();
        s.mounts.push(Mount::Bind {
            source: "/home/u/../../etc".into(),
            dest: "/x".into(),
            writable: true,
        });
        assert!(matches!(s.validate(), Err(SpecError::Invalid(_))));
    }

    #[test]
    fn recusa_comando_vazio_e_variavel_invalida() {
        let mut s = sample();
        s.command.clear();
        assert!(matches!(s.validate(), Err(SpecError::Invalid(_))));
        let mut s = sample();
        s.env.push(EnvVar {
            name: "A=B".into(),
            value: String::new(),
        });
        assert!(matches!(s.validate(), Err(SpecError::Invalid(_))));
    }
}
