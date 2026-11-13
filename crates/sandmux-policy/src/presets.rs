//! Presets embutidos: o que cada harness precisa para rodar confinado.

use std::path::{Path, PathBuf};

use sandmux_spec::Mount;

use crate::PolicyError;
use crate::host::Host;

/// Nomes dos presets, na ordem em que aparecem nas mensagens.
pub const PRESETS: &[&str] = &["opencode", "hermes", "shell"];

/// O que um preset acrescenta à baseline.
#[derive(Debug, Clone, Default)]
pub struct PresetPlan {
    /// Código do harness, em somente leitura.
    pub mounts: Vec<Mount>,
    /// Pastas com camada de escrita (ferramentas que o harness baixa).
    pub layered: Vec<PathBuf>,
    pub env: Vec<(String, String)>,
    pub command: Vec<String>,
    /// Valor de `SHELL` dentro do sandbox.
    pub shell: PathBuf,
}

pub fn exists(name: &str) -> bool {
    PRESETS.contains(&name)
}

/// O preset tem estado real que pode ser importado.
pub fn importable(name: &str) -> bool {
    matches!(name, "opencode" | "hermes")
}

/// Shell de sistema para o sandbox: o do usuário quando está nas pastas de
/// sistema; senão, `bash`.
pub fn system_shell(host: &Host) -> PathBuf {
    let s = &host.user_shell;
    let in_system = ["/usr/", "/bin/"].iter().any(|p| s.starts_with(p));
    if in_system && host.sys(s).is_file() {
        s.clone()
    } else if host.sys("/usr/bin/bash").is_file() {
        PathBuf::from("/usr/bin/bash")
    } else {
        PathBuf::from("/bin/bash")
    }
}

fn ro(p: &Path) -> Mount {
    Mount::Bind {
        source: p.to_path_buf(),
        dest: p.to_path_buf(),
        writable: false,
    }
}

fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Provedores do opencode cuja chave pode vir por `--pass-env`.
fn opencode_provider(var: &str) -> Option<String> {
    let known = match var {
        "OPENROUTER_API_KEY" => "openrouter",
        "ANTHROPIC_API_KEY" => "anthropic",
        "OPENAI_API_KEY" => "openai",
        "GEMINI_API_KEY" | "GOOGLE_GENERATIVE_AI_API_KEY" => "google",
        "GROQ_API_KEY" => "groq",
        "DEEPSEEK_API_KEY" => "deepseek",
        "MISTRAL_API_KEY" => "mistral",
        "XAI_API_KEY" => "xai",
        _ => "",
    };
    if !known.is_empty() {
        return Some(known.into());
    }
    var.strip_suffix("_API_KEY")
        .filter(|p| !p.is_empty())
        .map(|p| p.to_ascii_lowercase().replace('_', "-"))
}

/// Configuração injetada no opencode para que a chave passada por
/// `--pass-env` vença o valor falso do `auth.json` importado.
pub fn opencode_config_content(pass_env: &[String]) -> Option<String> {
    let mut providers = serde_like::Object::default();
    for var in pass_env {
        if let Some(p) = opencode_provider(var) {
            providers.push(
                p,
                format!("{{\"options\":{{\"apiKey\":\"{{env:{var}}}\"}}}}"),
            );
        }
    }
    (!providers.is_empty()).then(|| format!("{{\"provider\":{}}}", providers.render()))
}

/// JSON mínimo sem dependência: objeto com valores já serializados.
mod serde_like {
    #[derive(Default)]
    pub struct Object(Vec<(String, String)>);
    impl Object {
        pub fn push(&mut self, k: String, raw: String) {
            if !self.0.iter().any(|(e, _)| *e == k) {
                self.0.push((k, raw));
            }
        }
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
        pub fn render(&self) -> String {
            let body: Vec<String> = self.0.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
            format!("{{{}}}", body.join(","))
        }
    }
}

pub fn plan(
    name: &str,
    host: &Host,
    args: &[String],
    pass_env: &[String],
) -> Result<PresetPlan, PolicyError> {
    match name {
        "opencode" => {
            let exe = host.h(".opencode/bin/opencode");
            if !executable(&exe) {
                return Err(PolicyError::NotInstalled {
                    harness: name.into(),
                    searched: vec![exe],
                    hint: "instale o opencode no host com: curl -fsSL https://opencode.ai/install | bash".into(),
                });
            }
            let mut env = vec![("OPENCODE_DISABLE_AUTOUPDATE".to_string(), "1".to_string())];
            if let Some(cfg) = opencode_config_content(pass_env) {
                env.push(("OPENCODE_CONFIG_CONTENT".into(), cfg));
            }
            Ok(PresetPlan {
                mounts: vec![ro(&host.h(".opencode"))],
                layered: vec![],
                env,
                command: std::iter::once(exe.to_string_lossy().into_owned())
                    .chain(args.iter().cloned())
                    .collect(),
                shell: system_shell(host),
            })
        }
        "hermes" => {
            let code = host.h(".hermes/hermes-agent");
            let wrapper = host.h(".local/bin/hermes");
            if !code.is_dir() || !executable(&wrapper) {
                return Err(PolicyError::NotInstalled {
                    harness: name.into(),
                    searched: vec![code, wrapper],
                    hint: "instale o hermes no host seguindo a documentação oficial do projeto e rode `hermes` uma vez fora do sandbox".into(),
                });
            }
            let mut mounts = vec![ro(&code)];
            for f in ["hermes", "hermes-agent", "hermes-acp"] {
                let p = host.h(&format!(".local/bin/{f}"));
                if p.is_file() {
                    mounts.push(ro(&p));
                }
            }
            // Ferramentas baixadas (Python, Node, navegador) e ambientes de
            // dependências da instalação: lidos do host, escritos na camada.
            let layered = [".hermes/tools", ".hermes/installs"]
                .iter()
                .map(|r| host.h(r))
                .filter(|p| p.is_dir())
                .collect();
            Ok(PresetPlan {
                mounts,
                layered,
                env: vec![],
                command: std::iter::once(wrapper.to_string_lossy().into_owned())
                    .chain(args.iter().cloned())
                    .collect(),
                shell: system_shell(host),
            })
        }
        "shell" => {
            let shell = system_shell(host);
            Ok(PresetPlan {
                mounts: vec![],
                layered: vec![],
                env: vec![],
                command: std::iter::once(shell.to_string_lossy().into_owned())
                    .chain(args.iter().cloned())
                    .collect(),
                shell,
            })
        }
        other => Err(PolicyError::UnknownHarness(other.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_do_opencode_aponta_para_a_variavel() {
        let c = opencode_config_content(&["OPENROUTER_API_KEY".into(), "OUTRA".into()]).unwrap();
        assert_eq!(
            c,
            r#"{"provider":{"openrouter":{"options":{"apiKey":"{env:OPENROUTER_API_KEY}"}}}}"#
        );
        assert!(opencode_config_content(&["HTTP_PROXY".into()]).is_none());
        assert!(
            opencode_config_content(&["FOO_BAR_API_KEY".into()])
                .unwrap()
                .contains("\"foo-bar\"")
        );
    }
}
