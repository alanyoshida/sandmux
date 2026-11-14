//! Mensagens de controle e a saudação.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Tamanho de um terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

impl Size {
    /// O menor número de colunas e o menor de linhas entre vários tamanhos.
    pub fn min_of(sizes: impl IntoIterator<Item = Size>) -> Option<Size> {
        sizes.into_iter().reduce(|a, b| Size {
            cols: a.cols.min(b.cols),
            rows: a.rows.min(b.rows),
        })
    }
}

/// Opções de uma sessão, as mesmas de `sandmux run`. Guardadas na lista de
/// sessões: nunca contêm valores de variáveis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchOptions {
    pub harness: String,
    /// Pasta de trabalho, absoluta.
    pub workdir: PathBuf,
    pub ro: Vec<PathBuf>,
    pub rw: Vec<PathBuf>,
    /// Nomes das variáveis de `--pass-env` dadas no `run`.
    pub pass_env: Vec<String>,
    /// Perfil nomeado (`--profile`).
    #[serde(default)]
    pub profile: Option<String>,
    /// Opções de liga e desliga dadas no `run`: `None` quando não foram
    /// dadas, e a configuração decide.
    pub repo: Option<bool>,
    pub kvm: Option<bool>,
    pub gpu: Option<bool>,
    pub allow_clipboard: Option<bool>,
    pub args: Vec<String>,
}

impl LaunchOptions {
    pub fn new(harness: &str) -> Self {
        Self {
            harness: harness.into(),
            workdir: PathBuf::from("/"),
            ro: vec![],
            rw: vec![],
            pass_env: vec![],
            profile: None,
            repo: None,
            kvm: None,
            gpu: None,
            allow_clipboard: None,
            args: vec![],
        }
    }
}

/// Estado de uma sessão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "estado", rename_all = "snake_case")]
pub enum SessionState {
    Viva,
    Encerrada { codigo: i32 },
    Parada { motivo: String },
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Viva => write!(f, "viva"),
            SessionState::Encerrada { codigo } => write!(f, "encerrada ({codigo})"),
            SessionState::Parada { motivo } => write!(f, "parada: {motivo}"),
        }
    }
}

/// Uma linha de `sandmux ls`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: u32,
    pub name: String,
    pub harness: String,
    pub workdir: PathBuf,
    pub state: SessionState,
    pub clients: usize,
    /// Nomes das variáveis de `pass-env` do último lançamento (nunca os
    /// valores).
    pub pass_env: Vec<String>,
    /// Opções dadas no `run` e o perfil nomeado, para o reinício resolver a
    /// política de novo.
    pub opts: LaunchOptions,
}

/// Mensagens de controle, nos dois sentidos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Msg {
    // --- saudação
    Hello {
        protocol: u32,
        version: String,
        data_dir: PathBuf,
        /// Pasta de configuração do cliente (protocolo 2).
        #[serde(default)]
        config_dir: PathBuf,
    },
    Welcome {
        protocol: u32,
        version: String,
        pid: u32,
        start_mode: String,
    },
    Refused {
        message: String,
    },

    // --- pedidos do cliente
    /// Cria uma sessão. `vars` são a lista segura, o `SHELL` e os valores de
    /// `--pass-env`, só em memória.
    Create {
        opts: LaunchOptions,
        vars: BTreeMap<String, String>,
        size: Size,
        attach: bool,
    },
    Restart {
        id: u32,
        vars: BTreeMap<String, String>,
        size: Size,
        attach: bool,
    },
    Attach {
        id: Option<u32>,
        size: Size,
    },
    Detach {
        id: u32,
    },
    Kill {
        id: u32,
    },
    List,
    Resize {
        size: Size,
    },
    Status,
    Stop,

    // --- respostas e eventos do daemon
    Ok,
    Error {
        message: String,
    },
    Warning {
        message: String,
    },
    Created {
        id: u32,
        name: String,
    },
    Sessions {
        sessions: Vec<SessionInfo>,
    },
    Attached {
        id: u32,
        name: String,
    },
    /// O reenvio do histórico e da tela terminou.
    Replayed,
    Exited {
        id: u32,
        code: i32,
    },
    Detached {
        reason: String,
    },
    StatusInfo {
        pid: u32,
        protocol: u32,
        version: String,
        start_mode: String,
        sessions: usize,
        socket: PathBuf,
    },
}

/// Resposta do daemon à saudação: a mensagem a enviar e se a conexão segue.
pub fn answer_hello(
    hello: &Msg,
    protocol: u32,
    version: &str,
    pid: u32,
    start_mode: &str,
    data_dir: &std::path::Path,
    config_dir: &std::path::Path,
) -> (Msg, bool) {
    match hello {
        Msg::Hello {
            protocol: p,
            data_dir: d,
            config_dir: c,
            ..
        } => {
            if *p != protocol {
                // O cliente compara e explica; o daemon só encerra a conexão.
                let welcome = Msg::Welcome {
                    protocol,
                    version: version.into(),
                    pid,
                    start_mode: start_mode.into(),
                };
                (welcome, false)
            } else if d != data_dir {
                (
                    Msg::Refused {
                        message: format!(
                            "o daemon usa o diretório de dados {} e este terminal usa {}; defina o mesmo XDG_DATA_HOME nos dois ou rode `sandmux daemon restart` neste terminal",
                            data_dir.display(),
                            d.display()
                        ),
                    },
                    false,
                )
            } else if c != config_dir {
                (
                    Msg::Refused {
                        message: format!(
                            "o daemon usa a pasta de configuração {} e este terminal usa {}; defina o mesmo XDG_CONFIG_HOME nos dois ou rode `sandmux daemon restart` neste terminal",
                            config_dir.display(),
                            c.display()
                        ),
                    },
                    false,
                )
            } else {
                (
                    Msg::Welcome {
                        protocol,
                        version: version.into(),
                        pid,
                        start_mode: start_mode.into(),
                    },
                    true,
                )
            }
        }
        _ => (
            Msg::Refused {
                message: "a conexão precisa começar pela saudação".into(),
            },
            false,
        ),
    }
}

/// Dados do daemon depois de uma saudação aceita.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonInfo {
    pub protocol: u32,
    pub version: String,
    pub pid: u32,
    pub start_mode: String,
}

/// Confere, no cliente, a resposta à saudação.
pub fn check_welcome(reply: &Msg, protocol: u32, version: &str) -> Result<DaemonInfo, String> {
    match reply {
        Msg::Welcome {
            protocol: p,
            version: v,
            pid,
            start_mode,
        } => {
            if *p != protocol {
                Err(format!(
                    "o daemon em execução usa o protocolo {p} (sandmux {v}) e este cliente usa o protocolo {protocol} (sandmux {version}); rode `sandmux daemon restart` para trocar o daemon pela versão instalada (os agentes são relançados e o histórico de tela se perde)"
                ))
            } else {
                Ok(DaemonInfo {
                    protocol: *p,
                    version: v.clone(),
                    pid: *pid,
                    start_mode: start_mode.clone(),
                })
            }
        }
        Msg::Refused { message } => Err(message.clone()),
        other => Err(format!("resposta inesperada do daemon: {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn hello(p: u32, dir: &str) -> Msg {
        hello_cfg(p, dir, "/c")
    }

    fn hello_cfg(p: u32, dir: &str, cfg: &str) -> Msg {
        Msg::Hello {
            protocol: p,
            version: "0.2.0".into(),
            data_dir: dir.into(),
            config_dir: cfg.into(),
        }
    }

    #[test]
    fn pasta_de_configuracao_diferente_recusada() {
        let (reply, ok) = answer_hello(
            &hello_cfg(1, "/d", "/tmp/cfg/sandmux"),
            1,
            "0.1.0",
            42,
            "direct",
            Path::new("/d"),
            Path::new("/home/ana/.config/sandmux"),
        );
        assert!(!ok);
        let err = check_welcome(&reply, 1, "0.1.0").unwrap_err();
        assert!(
            err.contains("/tmp/cfg/sandmux") && err.contains("/home/ana/.config/sandmux"),
            "{err}"
        );
    }

    #[test]
    fn opcoes_sem_valor_e_perfil() {
        let mut o = LaunchOptions::new("shell");
        o.kvm = Some(false);
        o.profile = Some("vm".into());
        let back: LaunchOptions =
            serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap();
        assert_eq!(back, o);
        assert_eq!(back.gpu, None);
    }

    #[test]
    fn saudacao_aceita() {
        let (reply, ok) = answer_hello(
            &hello(1, "/d"),
            1,
            "0.1.0",
            42,
            "direct",
            Path::new("/d"),
            Path::new("/c"),
        );
        assert!(ok);
        let info = check_welcome(&reply, 1, "0.2.0").unwrap();
        assert_eq!(info.pid, 42);
        assert_eq!(info.start_mode, "direct");
    }

    #[test]
    fn protocolo_diferente_recusado_dos_dois_lados() {
        let (reply, ok) = answer_hello(
            &hello(2, "/d"),
            1,
            "0.1.0",
            42,
            "systemd",
            Path::new("/d"),
            Path::new("/c"),
        );
        assert!(!ok, "o daemon encerra a conexão");
        let err = check_welcome(&reply, 2, "0.2.0").unwrap_err();
        assert!(
            err.contains("protocolo 1 (sandmux 0.1.0)")
                && err.contains("protocolo 2 (sandmux 0.2.0)")
                && err.contains("sandmux daemon restart"),
            "{err}"
        );
    }

    #[test]
    fn diretorio_de_dados_diferente_recusado() {
        let (reply, ok) = answer_hello(
            &hello(1, "/b"),
            1,
            "0.1.0",
            42,
            "direct",
            Path::new("/a"),
            Path::new("/c"),
        );
        assert!(!ok);
        let err = check_welcome(&reply, 1, "0.1.0").unwrap_err();
        assert!(err.contains("/a") && err.contains("/b"), "{err}");
    }

    #[test]
    fn primeira_mensagem_precisa_ser_saudacao() {
        let (reply, ok) = answer_hello(
            &Msg::List,
            1,
            "0.1.0",
            1,
            "direct",
            Path::new("/a"),
            Path::new("/c"),
        );
        assert!(!ok);
        assert!(matches!(reply, Msg::Refused { .. }));
    }

    #[test]
    fn menor_tamanho() {
        let s = Size::min_of([
            Size {
                cols: 200,
                rows: 30,
            },
            Size {
                cols: 100,
                rows: 50,
            },
        ]);
        assert_eq!(
            s,
            Some(Size {
                cols: 100,
                rows: 30
            })
        );
        assert_eq!(Size::min_of([]), None);
    }
}
