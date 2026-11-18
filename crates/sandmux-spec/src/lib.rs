//! Especificação de lançamento do sandmux.
//!
//! É o contrato entre quem compõe a política (o cliente `sandmux` e, depois, o
//! daemon) e o lançador `sandmux-run`. O lançador não conhece harnesses nem
//! política: monta exatamente o que a especificação descreve e executa o
//! comando. O formato é JSON, com um campo de versão; um campo desconhecido ou
//! uma versão diferente fazem a leitura falhar.

pub mod files;
pub mod lang;
pub mod net;
pub mod window;

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
    /// Rede do sandbox. Sem ela, o sandbox usa a rede do host (só a visão do
    /// Docker privado e os testes do lançador).
    #[serde(default)]
    pub network: Option<NetworkSpec>,
    /// Registro dos acessos às pastas do agente. Sem ele, as aberturas
    /// continuam passando pelo supervisor, sem registro.
    #[serde(default)]
    pub files: Option<FilesSpec>,
    /// Repassador do barramento do portal: o processo 1 cria `listen` dentro
    /// do sandbox e liga cada conexão a `target`, o socket do daemon montado
    /// no sandbox. Um socket criado lá dentro tem um caminho que o AppArmor
    /// dos programas com perfil (o `notify-send`) consegue resolver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bus_relay: Option<BusRelay>,
}

/// O repassador do barramento do portal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusRelay {
    pub listen: PathBuf,
    pub target: PathBuf,
}

/// Lugar das pastas novas dentro do sandbox.
pub const FOLDERS_DIR: &str = "/mnt/sandmux";

/// Uma pasta do host montada em `/mnt/sandmux/<name>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveFolder {
    pub name: String,
    pub path: PathBuf,
    pub rw: bool,
}

impl LiveFolder {
    /// Caminho dentro do sandbox.
    pub fn dest(&self) -> PathBuf {
        Path::new(FOLDERS_DIR).join(&self.name)
    }
}

/// O texto do `/mnt/sandmux/PASTAS.txt`.
pub fn folders_text(list: &[LiveFolder]) -> String {
    let mut s = String::from(
        "# Pastas do host liberadas pelo usuário nesta sessão (sandmux).\n# nome\tmodo\tpasta no host\n",
    );
    for f in list {
        s.push_str(&format!(
            "{}\t{}\t{}\n",
            f.name,
            if f.rw {
                "leitura e escrita"
            } else {
                "somente leitura"
            },
            f.path.display()
        ));
    }
    s
}

/// O registro de arquivos de um lançamento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesSpec {
    /// Pasta do registro, no host.
    pub log_dir: PathBuf,
    /// Etiqueta deste lançamento.
    pub tag: String,
    /// Pasta de trabalho.
    pub workdir: PathBuf,
    /// Pastas do agente, na visão do agente: só os acessos a elas vão para o
    /// registro.
    pub roots: Vec<PathBuf>,
    /// Pastas dentro delas que ficam fora do registro (caches e toolchains
    /// montados no HOME).
    #[serde(default)]
    pub exclude: Vec<PathBuf>,
    /// Política de arquivos inicial.
    #[serde(default)]
    pub policy: files::FilePolicy,
    /// Arquivo com a política atual, regravado pelo daemon; o supervisor o
    /// relê. Vazio no `run` sem terminal.
    #[serde(default)]
    pub policy_file: Option<PathBuf>,
    /// Pasta dos arquivos criados pelo agente e dos caminhos conhecidos, por
    /// pasta de trabalho.
    #[serde(default)]
    pub state_dir: Option<PathBuf>,
    /// Socket de controle do supervisor (pastas ao vivo), só nas sessões do
    /// daemon.
    #[serde(default)]
    pub control_socket: Option<PathBuf>,
    /// Pastas montadas no lançamento em `/mnt/sandmux`.
    #[serde(default)]
    pub folders: Vec<LiveFolder>,
    /// O supervisor também recebe `execve` e `execveat` e registra as
    /// execuções (o recuo do registro de execuções pelo kernel).
    #[serde(default)]
    pub exec_log: bool,
}

/// Uma linha do registro de arquivos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub ts: String,
    pub tag: String,
    pub workdir: PathBuf,
    /// Caminho na visão do agente.
    pub path: PathBuf,
    /// `r`, `w`, `rw` ou `create`.
    pub access: String,
    /// Motivo do bloqueio, ou `None`.
    pub blocked: Option<String>,
    /// O caminho apareceu pela primeira vez nesta pasta de trabalho.
    #[serde(default)]
    pub new: bool,
    /// Os argumentos de uma execução (`access` = `exec`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// A fonte de uma execução: `kernel` ou `supervisor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

/// Modo de rede.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetMode {
    /// Tudo pelo gateway, registrado.
    Open,
    /// Só o `lo`.
    None,
}

/// Rede de um sandbox: namespace próprio, TUN e gateway no supervisor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkSpec {
    pub mode: NetMode,
    /// Pasta do registro de rede, no host.
    pub log_dir: PathBuf,
    /// Etiqueta deste lançamento no registro.
    pub tag: String,
    /// Pasta de trabalho, para o registro e os domínios conhecidos.
    pub workdir: PathBuf,
    /// Servidores de nome do host, para o DNS do gateway.
    pub nameservers: Vec<String>,
    /// Política compilada do lançamento.
    #[serde(default)]
    pub policy: net::NetPolicy,
    /// Arquivo com a política atual, regravado pelo daemon quando ela muda;
    /// o gateway o relê. Vazio no `run` sem terminal.
    #[serde(default)]
    pub policy_file: Option<PathBuf>,
    /// Arquivo onde o supervisor lista as portas publicadas no host.
    #[serde(default)]
    pub ports_file: Option<PathBuf>,
    /// Na visão do Docker: o socket do daemon para onde vai o TUN (o gateway
    /// roda no daemon, não no supervisor).
    #[serde(default)]
    pub tun_socket: Option<PathBuf>,
    /// CA do lançamento, para decifrar o TLS (a chave só vive no memfd da
    /// especificação e na memória do supervisor).
    #[serde(default)]
    pub tls: Option<TlsSpec>,
    /// Socket do daemon e as pastas da saudação, para o gateway pedir os
    /// valores reais dos segredos (só nas sessões do daemon).
    #[serde(default)]
    pub vault: Option<VaultAccess>,
    /// Harness da aba: só os endpoints de login dele são capturados, e só os
    /// segredos de login dele são trocados.
    #[serde(default)]
    pub harness: String,
}

/// Como o gateway fala com o daemon para pedir os segredos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultAccess {
    pub socket: PathBuf,
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
}

/// CA de um lançamento e a CA adicional dos destinos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsSpec {
    pub ca_cert_pem: String,
    pub ca_key_pem: String,
    /// CA a mais para verificar os destinos (servidor local com HTTPS).
    #[serde(default)]
    pub upstream_ca_pem: Option<String>,
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
    /// `/sys` novo, do namespace de rede de quem monta (visão do Docker).
    Sysfs { dest: PathBuf },
    /// cgroup2 novo, com escrita, cuja raiz é a do cgroup namespace de quem
    /// monta (visão do Docker, dentro do escopo da pasta).
    Cgroup2 { dest: PathBuf },
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
    /// Nomes de chamadas negadas com `ENOSYS` ("o kernel não tem"), e não com
    /// `EPERM`: para programas que só toleram a falta da chamada, como o
    /// `runc` com o chaveiro do kernel (visão do Docker).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enosys_syscalls: Vec<String>,
}

/// Erros de leitura e validação.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SpecError {
    Malformed(String),
    UnsupportedVersion { found: String },
    Invalid(String),
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let supported = SPEC_VERSION;
        match self {
            SpecError::Malformed(m) => f.write_str(&crate::trf!(
                "especificação malformada: {m}",
                "malformed specification: {m}"
            )),
            SpecError::UnsupportedVersion { found } => f.write_str(&crate::trf!(
                "versão de especificação não suportada: recebida {found}, suportada {supported}",
                "unsupported specification version: got {found}, supported {supported}"
            )),
            SpecError::Invalid(m) => f.write_str(&crate::trf!(
                "especificação inválida: {m}",
                "invalid specification: {m}"
            )),
        }
    }
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
            None => Some(crate::tr!("ausente", "missing").to_string()),
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
            return Err(SpecError::Invalid(
                crate::tr!("comando vazio", "empty command").into(),
            ));
        }
        check_path("cwd", &self.cwd)?;
        for m in &self.mounts {
            for (label, p) in m.paths() {
                check_path(label, p)?;
            }
            if let Mount::Symlink { target, .. } = m
                && target.as_os_str().is_empty()
            {
                return Err(SpecError::Invalid(
                    crate::tr!(
                        "link simbólico sem destino",
                        "symbolic link without a target"
                    )
                    .into(),
                ));
            }
        }
        for p in self.landlock.read.iter().chain(&self.landlock.write) {
            check_path("landlock", p)?;
        }
        if let Some(r) = &self.bus_relay {
            check_path("bus_relay", &r.listen)?;
            check_path("bus_relay", &r.target)?;
        }
        for v in &self.env {
            if v.name.is_empty() || v.name.contains('=') || v.name.contains('\0') {
                return Err(SpecError::Invalid(crate::trf!(
                    "nome de variável inválido: {:?}",
                    "invalid variable name: {:?}",
                    v.name
                )));
            }
            if v.value.contains('\0') {
                return Err(SpecError::Invalid(crate::trf!(
                    "valor com byte nulo na variável {}",
                    "value with a null byte in variable {}",
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
            | Mount::Devpts { dest }
            | Mount::Sysfs { dest }
            | Mount::Cgroup2 { dest } => dest,
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
        return Err(SpecError::Invalid(crate::trf!(
            "{label}: caminho não absoluto: {}",
            "{label}: path is not absolute: {}",
            p.display()
        )));
    }
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(SpecError::Invalid(crate::trf!(
            "{label}: caminho com '..': {}",
            "{label}: path with '..': {}",
            p.display()
        )));
    }
    if p.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(SpecError::Invalid(crate::trf!(
            "{label}: caminho com byte nulo",
            "{label}: path with a null byte"
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
                Mount::Cgroup2 {
                    dest: "/sys/fs/cgroup".into(),
                },
            ],
            landlock: LandlockRules {
                read: vec!["/".into()],
                write: vec!["/tmp".into()],
            },
            seccomp: SeccompRules {
                deny_syscalls: vec!["mount".into()],
                deny_ioctls: vec!["TIOCSTI".into()],
                enosys_syscalls: vec!["keyctl".into()],
            },
            env: vec![EnvVar {
                name: "PATH".into(),
                value: "/usr/bin".into(),
            }],
            command: vec!["true".into()],
            cwd: "/tmp".into(),
            network: None,
            files: None,
            bus_relay: None,
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
    fn cgroup2_com_caminho_relativo_e_recusado() {
        let mut s = sample();
        s.mounts.push(Mount::Cgroup2 {
            dest: "sys/fs/cgroup".into(),
        });
        assert!(matches!(s.validate(), Err(SpecError::Invalid(_))));
        let json = sample().to_json();
        assert!(json.contains(r#""type":"cgroup2""#), "{json}");
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
