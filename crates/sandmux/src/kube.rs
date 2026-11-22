//! As identidades de Kubernetes: o kubeconfig do host lido pelo `kubectl`
//! do host, os plugins `exec` rodados fora do sandbox, e o real (o token ou
//! o certificado de cliente, com o API server) que só o gateway recebe.

use std::time::Duration;

use base64::Engine;
use sandmux_proto::msg::{CloudAdd, KubeTarget};
use sandmux_spec::{tr, trf};
use serde::{Deserialize, Serialize};

use crate::cloud::{GcloudErr, Real, now, run_host};

/// Troca o `kubectl` do host (testes).
pub const KUBECTL_ENV: &str = "SANDMUX_KUBECTL";
/// Por quanto vale um real sem validade: com a margem de 5 minutos do
/// cache, ele é relido a cada 10 minutos.
pub const NO_EXPIRY: u64 = 15 * 60;
const KUBECTL_TIMEOUT: Duration = Duration::from_secs(30);
const EXEC_TIMEOUT: Duration = Duration::from_secs(60);

pub fn kubectl_bin() -> String {
    std::env::var(KUBECTL_ENV)
        .ok()
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "kubectl".into())
}

/// O `kubectl` está no host?
pub fn kubectl_present() -> bool {
    let bin = kubectl_bin();
    if bin.contains('/') {
        return std::path::Path::new(&bin).is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(&bin).is_file()))
}

/// O `KUBECONFIG` do usuário, ou o caminho padrão.
pub fn host_kubeconfig() -> String {
    if let Some(k) = std::env::var("KUBECONFIG").ok().filter(|k| !k.is_empty()) {
        return k;
    }
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join(".kube/config")
        .display()
        .to_string()
}

/// O kubeconfig do host visto pelo `kubectl` (com `--flatten`), inteiro ou
/// só um contexto.
pub fn view(kubeconfig: &str, context: Option<&str>) -> Result<serde_json::Value, String> {
    let mut args = vec!["config", "view", "--raw", "--flatten", "-o", "json"];
    if let Some(c) = context {
        args.extend(["--minify", "--context", c]);
    }
    let env = vec![("KUBECONFIG".to_string(), kubeconfig.to_string())];
    let out = run_host(&kubectl_bin(), &args, &env, KUBECTL_TIMEOUT).map_err(|e| match e {
        GcloudErr::Missing => tr!(
            "o kubectl não está instalado no host",
            "kubectl is not installed on the host"
        )
        .to_string(),
        GcloudErr::Failed(m) => trf!(
            "o kubectl do host falhou: {m}",
            "the host kubectl failed: {m}"
        ),
    })?;
    serde_json::from_str(&out).map_err(|e| {
        trf!(
            "a saída do kubectl não é um JSON válido: {e}",
            "the kubectl output is not valid JSON: {e}"
        )
    })
}

fn s<'a>(v: &'a serde_json::Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or_default()
}

/// Um item de `clusters`, `users` ou `contexts` pelo nome.
fn named<'a>(
    view: &'a serde_json::Value,
    list: &str,
    inner: &str,
    name: &str,
) -> Option<&'a serde_json::Value> {
    view.get(list)?
        .as_array()?
        .iter()
        .find(|x| s(x, "name") == name)
        .and_then(|x| x.get(inner))
}

/// Um contexto do kubeconfig, como o comando o mostra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    pub name: String,
    pub server: String,
    pub namespace: String,
}

/// Os contextos do kubeconfig e o contexto atual.
pub fn contexts(view: &serde_json::Value) -> (Vec<Context>, String) {
    let list = view
        .get("contexts")
        .and_then(|c| c.as_array())
        .map(|a| {
            a.iter()
                .map(|c| {
                    let ctx = c.get("context").cloned().unwrap_or_default();
                    let cluster = named(view, "clusters", "cluster", s(&ctx, "cluster"));
                    Context {
                        name: s(c, "name").to_string(),
                        server: cluster
                            .map(|c| s(c, "server").to_string())
                            .unwrap_or_default(),
                        namespace: s(&ctx, "namespace").to_string(),
                    }
                })
                .filter(|c| !c.name.is_empty())
                .collect()
        })
        .unwrap_or_default();
    (list, s(view, "current-context").to_string())
}

/// O que um contexto dá para chegar ao API server.
#[derive(Default)]
pub struct Material {
    pub target: KubeTarget,
    pub token: String,
    pub token_file: String,
    /// O plugin `exec` do usuário, como está no kubeconfig.
    pub exec: Option<serde_json::Value>,
    /// O cluster, para o `KUBERNETES_EXEC_INFO`.
    pub cluster: serde_json::Value,
    pub auth_provider: bool,
    pub basic: bool,
}

fn b64(v: &str) -> Result<String, String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(v.trim())
        .map_err(|e| trf!("dado em base64 inválido: {e}", "invalid base64 data: {e}"))?;
    String::from_utf8(raw).map_err(|e| e.to_string())
}

/// O host (sem colchetes) e a autoridade de um servidor `https://…`.
pub fn server_host(server: &str) -> Option<(String, String)> {
    sandmux_spec::net::kube_server(server).map(|k| (k.host, k.authority))
}

/// O material do contexto `context` de uma visão do `kubectl`.
pub fn material(view: &serde_json::Value, context: &str) -> Result<Material, String> {
    let ctx = named(view, "contexts", "context", context).ok_or_else(|| {
        trf!(
            "o contexto {context} não existe no kubeconfig do host",
            "context {context} does not exist in the host kubeconfig"
        )
    })?;
    let cluster = named(view, "clusters", "cluster", s(ctx, "cluster"))
        .cloned()
        .unwrap_or_default();
    let server = s(&cluster, "server").to_string();
    let Some((host, _)) = server_host(&server) else {
        return Err(trf!(
            "o servidor do contexto {context} não é https: {server:?}",
            "the server of context {context} is not https: {server:?}"
        ));
    };
    let mut m = Material {
        target: KubeTarget {
            server: server.clone(),
            ca_pem: match s(&cluster, "certificate-authority-data") {
                "" => String::new(),
                d => b64(d)?,
            },
            server_name: match s(&cluster, "tls-server-name") {
                "" => host,
                n => n.to_string(),
            },
            insecure: cluster
                .get("insecure-skip-tls-verify")
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
            ..Default::default()
        },
        cluster: cluster.clone(),
        ..Default::default()
    };
    let user = named(view, "users", "user", s(ctx, "user"))
        .cloned()
        .unwrap_or_default();
    m.token = s(&user, "token").to_string();
    m.token_file = s(&user, "tokenFile").to_string();
    if let (c, k) = (
        s(&user, "client-certificate-data"),
        s(&user, "client-key-data"),
    ) && !c.is_empty()
        && !k.is_empty()
    {
        m.target.cert_pem = b64(c)?;
        m.target.key_pem = b64(k)?;
    }
    m.exec = user.get("exec").filter(|e| !e.is_null()).cloned();
    m.auth_provider = user.get("auth-provider").is_some_and(|a| !a.is_null());
    m.basic = !s(&user, "username").is_empty() || !s(&user, "password").is_empty();
    Ok(m)
}

/// Recusa o que o sandmux não leva ao API server: o `auth-provider` antigo,
/// usuário e senha e, numa cópia, o plugin `exec` (que só roda no host).
pub fn check(m: &Material, context: &str, copy: bool) -> Result<(), String> {
    if m.auth_provider {
        return Err(trf!(
            "o contexto {context} usa um auth-provider, que o sandmux não aceita; troque por um plugin exec",
            "context {context} uses an auth-provider, which sandmux does not accept; switch to an exec plugin"
        ));
    }
    if m.basic && m.token.is_empty() && m.target.cert_pem.is_empty() && m.exec.is_none() {
        return Err(trf!(
            "o contexto {context} usa usuário e senha, que o sandmux não aceita",
            "context {context} uses a username and password, which sandmux does not accept"
        ));
    }
    if copy && m.exec.is_some() {
        return Err(trf!(
            "o contexto {context} usa um plugin exec, que só roda no host: guarde a referência (sem --copy)",
            "context {context} uses an exec plugin, which only runs on the host: store the reference (without --copy)"
        ));
    }
    Ok(())
}

/// O que uma cópia guarda cifrado no cofre.
#[derive(Default, Serialize, Deserialize)]
pub struct Copied {
    pub target: KubeTarget,
    #[serde(default)]
    pub token: String,
}

/// O segredo de uma cópia: o destino e o token (o `tokenFile` lido agora).
pub fn copied(m: &Material) -> Result<String, String> {
    let token = if m.token.is_empty() && !m.token_file.is_empty() {
        read_token_file(&m.token_file)?
    } else {
        m.token.clone()
    };
    serde_json::to_string(&Copied {
        target: m.target.clone(),
        token,
    })
    .map_err(|e| e.to_string())
}

fn read_token_file(p: &str) -> Result<String, String> {
    std::fs::read_to_string(p)
        .map(|t| t.trim().to_string())
        .map_err(|e| trf!("ler o tokenFile {p}: {e}", "read the tokenFile {p}: {e}"))
}

/// O `KUBERNETES_EXEC_INFO` de um plugin: sem terminal, e com o cluster
/// quando o kubeconfig pede.
pub fn exec_info(exec: &serde_json::Value, cluster: &serde_json::Value) -> serde_json::Value {
    let mut spec = serde_json::json!({ "interactive": false });
    if exec.get("provideClusterInfo").and_then(|x| x.as_bool()) == Some(true) {
        let mut c = serde_json::Map::new();
        for k in [
            "server",
            "certificate-authority-data",
            "tls-server-name",
            "insecure-skip-tls-verify",
            "proxy-url",
        ] {
            if let Some(v) = cluster.get(k) {
                c.insert(k.to_string(), v.clone());
            }
        }
        if let Some(cfg) = cluster
            .get("extensions")
            .and_then(|e| e.as_array())
            .and_then(|a| {
                a.iter()
                    .find(|x| s(x, "name") == "client.authentication.k8s.io/exec")
            })
            .and_then(|x| x.get("extension"))
        {
            c.insert("config".into(), cfg.clone());
        }
        spec["cluster"] = serde_json::Value::Object(c);
    }
    serde_json::json!({
        "apiVersion": match s(exec, "apiVersion") {
            "" => "client.authentication.k8s.io/v1",
            v => v,
        },
        "kind": "ExecCredential",
        "spec": spec,
    })
}

/// O que um plugin devolveu: o token ou o certificado, e a validade.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExecOut {
    pub token: String,
    pub cert_pem: String,
    pub key_pem: String,
    pub expires: Option<u64>,
}

/// Lê a saída `ExecCredential` de um plugin.
pub fn parse_exec(out: &str) -> Result<ExecOut, String> {
    let v: serde_json::Value = serde_json::from_str(out).map_err(|e| {
        trf!(
            "a saída do plugin exec não é um ExecCredential: {e}",
            "the exec plugin output is not an ExecCredential: {e}"
        )
    })?;
    let st = v.get("status").cloned().unwrap_or_default();
    let o = ExecOut {
        token: s(&st, "token").to_string(),
        cert_pem: s(&st, "clientCertificateData").to_string(),
        key_pem: s(&st, "clientKeyData").to_string(),
        expires: crate::cloud::parse_rfc3339(s(&st, "expirationTimestamp")),
    };
    if o.token.is_empty() && (o.cert_pem.is_empty() || o.key_pem.is_empty()) {
        return Err(tr!(
            "o plugin exec não devolveu um token nem um certificado",
            "the exec plugin returned neither a token nor a certificate"
        )
        .into());
    }
    Ok(o)
}

/// Roda o plugin `exec` no host, com o `env` do kubeconfig e o
/// `KUBERNETES_EXEC_INFO`.
fn run_exec(exec: &serde_json::Value, cluster: &serde_json::Value) -> Result<ExecOut, String> {
    let cmd = s(exec, "command");
    if cmd.is_empty() {
        return Err(tr!(
            "o plugin exec não tem comando",
            "the exec plugin has no command"
        )
        .into());
    }
    let args: Vec<&str> = exec
        .get("args")
        .and_then(|a| a.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
        .unwrap_or_default();
    let mut env: Vec<(String, String)> = exec
        .get("env")
        .and_then(|e| e.as_array())
        .map(|a| {
            a.iter()
                .map(|x| (s(x, "name").to_string(), s(x, "value").to_string()))
                .filter(|(k, _)| !k.is_empty())
                .collect()
        })
        .unwrap_or_default();
    env.push((
        "KUBERNETES_EXEC_INFO".into(),
        exec_info(exec, cluster).to_string(),
    ));
    let out = run_host(cmd, &args, &env, EXEC_TIMEOUT).map_err(|e| exec_failure(cmd, &e))?;
    parse_exec(&out)
}

/// O motivo de uma falha do plugin: a última linha do erro dele, que
/// costuma dizer o que fazer no host (`aws sso login`).
pub(crate) fn exec_failure(cmd: &str, e: &GcloudErr) -> String {
    match e {
        GcloudErr::Missing => trf!(
            "o plugin exec {cmd} não está instalado no host",
            "the exec plugin {cmd} is not installed on the host"
        ),
        GcloudErr::Failed(m) => {
            let last = m
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim();
            trf!(
                "o plugin exec {cmd} falhou no host: {last}",
                "the exec plugin {cmd} failed on the host: {last}"
            )
        }
    }
}

/// O real de um material: o token (do kubeconfig, do `tokenFile` ou do
/// plugin) e o certificado de cliente, com a validade.
pub fn real_of(mut m: Material, now: u64) -> Result<Real, String> {
    let mut expires = now + NO_EXPIRY;
    let mut token = std::mem::take(&mut m.token);
    if let Some(exec) = &m.exec {
        let o = run_exec(exec, &m.cluster)?;
        token = o.token;
        if !o.cert_pem.is_empty() {
            m.target.cert_pem = o.cert_pem;
            m.target.key_pem = o.key_pem;
        }
        if let Some(e) = o.expires {
            expires = e;
        }
    } else if token.is_empty() && !m.token_file.is_empty() {
        token = read_token_file(&m.token_file)?;
    }
    Ok(Real {
        token,
        expires,
        kube: Some(std::mem::take(&mut m.target)),
        aws: None,
    })
}

/// Obtém o real de uma identidade de Kubernetes, fora do sandbox.
pub fn resolve(id: &CloudAdd) -> Result<Real, String> {
    match id.info.source.as_str() {
        "host" => {
            let ctx = id.info.configuration.as_str();
            let v = view(&id.info.kubeconfig, Some(ctx))?;
            let m = material(&v, ctx)?;
            check(&m, ctx, false)?;
            real_of(m, now())
        }
        "copy" => {
            let c: Copied = serde_json::from_str(&id.secret).map_err(|e| e.to_string())?;
            Ok(Real {
                token: c.token.clone(),
                expires: now() + NO_EXPIRY,
                kube: Some(c.target.clone()),
                aws: None,
            })
        }
        other => Err(trf!(
            "fonte de identidade desconhecida: {other}",
            "unknown identity source: {other}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(t: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(t)
    }

    fn sample() -> serde_json::Value {
        serde_json::json!({
            "current-context": "kind-dev",
            "clusters": [
                {"name": "kind-dev", "cluster": {"server": "https://127.0.0.1:41234", "certificate-authority-data": enc("CA-PEM")}},
                {"name": "eks", "cluster": {"server": "https://ABC.gr7.us-east-1.eks.amazonaws.com/prefixo", "tls-server-name": "api.eks", "insecure-skip-tls-verify": true}},
                {"name": "v6", "cluster": {"server": "https://[::1]:6443"}}
            ],
            "users": [
                {"name": "kind-dev", "user": {"client-certificate-data": enc("CERT"), "client-key-data": enc("CHAVE")}},
                {"name": "eks", "user": {"exec": {"command": "aws", "args": ["eks", "get-token"], "apiVersion": "client.authentication.k8s.io/v1beta1", "provideClusterInfo": true}}},
                {"name": "velho", "user": {"auth-provider": {"name": "gcp"}}},
                {"name": "senha", "user": {"username": "a", "password": "b"}}
            ],
            "contexts": [
                {"name": "kind-dev", "context": {"cluster": "kind-dev", "user": "kind-dev", "namespace": "app"}},
                {"name": "arn:aws:eks:us-east-1:1:cluster/prod", "context": {"cluster": "eks", "user": "eks"}},
                {"name": "velho", "context": {"cluster": "kind-dev", "user": "velho"}},
                {"name": "senha", "context": {"cluster": "v6", "user": "senha"}}
            ]
        })
    }

    #[test]
    fn contextos_e_material() {
        let v = sample();
        let (list, cur) = contexts(&v);
        assert_eq!(cur, "kind-dev");
        assert_eq!(list.len(), 4);
        assert_eq!(list[0].server, "https://127.0.0.1:41234");
        assert_eq!(list[0].namespace, "app");
        let m = material(&v, "kind-dev").unwrap();
        assert_eq!(m.target.ca_pem, "CA-PEM");
        assert_eq!(m.target.server_name, "127.0.0.1");
        assert_eq!(m.target.cert_pem, "CERT");
        assert_eq!(m.target.key_pem, "CHAVE");
        assert!(check(&m, "kind-dev", true).is_ok());
        let m = material(&v, "arn:aws:eks:us-east-1:1:cluster/prod").unwrap();
        assert_eq!(m.target.server_name, "api.eks");
        assert!(m.target.insecure);
        assert!(m.exec.is_some());
        assert!(check(&m, "eks", false).is_ok());
        assert!(check(&m, "eks", true).unwrap_err().contains("exec"));
        assert!(check(&material(&v, "velho").unwrap(), "velho", false).is_err());
        let m = material(&v, "senha").unwrap();
        assert_eq!(m.target.server_name, "::1");
        assert!(check(&m, "senha", false).is_err());
        assert!(material(&v, "nenhum").is_err());
    }

    #[test]
    fn servidor_e_host() {
        assert_eq!(
            server_host("https://h.example:6443/k8s/c-1"),
            Some(("h.example".into(), "h.example:6443".into()))
        );
        assert_eq!(
            server_host("https://[::1]:6443"),
            Some(("::1".into(), "[::1]:6443".into()))
        );
        assert_eq!(server_host("https://h"), Some(("h".into(), "h".into())));
        assert_eq!(server_host("http://h:80"), None);
        assert_eq!(server_host("https://h:porta"), None);
    }

    #[test]
    fn saida_do_plugin_exec() {
        let o = parse_exec(
            r#"{"kind":"ExecCredential","status":{"token":"k8s-real","expirationTimestamp":"2026-11-22T10:00:00Z"}}"#,
        )
        .unwrap();
        assert_eq!(o.token, "k8s-real");
        assert_eq!(
            o.expires,
            crate::cloud::parse_rfc3339("2026-11-22T10:00:00Z")
        );
        let o =
            parse_exec(r#"{"status":{"clientCertificateData":"C","clientKeyData":"K"}}"#).unwrap();
        assert_eq!(
            (o.cert_pem.as_str(), o.key_pem.as_str(), o.expires),
            ("C", "K", None)
        );
        assert!(parse_exec(r#"{"status":{}}"#).is_err());
        assert!(parse_exec("nada").is_err());
        let why = exec_failure(
            "aws",
            &GcloudErr::Failed("trace\nError: run `aws sso login`\n".into()),
        );
        assert!(why.contains("aws sso login"), "{why}");
    }

    #[test]
    fn info_do_plugin() {
        let v = sample();
        let m = material(&v, "arn:aws:eks:us-east-1:1:cluster/prod").unwrap();
        let i = exec_info(m.exec.as_ref().unwrap(), &m.cluster);
        assert_eq!(i["apiVersion"], "client.authentication.k8s.io/v1beta1");
        assert_eq!(i["spec"]["interactive"], false);
        assert_eq!(
            i["spec"]["cluster"]["server"],
            "https://ABC.gr7.us-east-1.eks.amazonaws.com/prefixo"
        );
    }

    #[test]
    fn real_sem_validade_e_copia() {
        let v = sample();
        let m = material(&v, "kind-dev").unwrap();
        let secret = copied(&m).unwrap();
        let r = real_of(m, 1000).unwrap();
        assert_eq!(r.expires, 1000 + NO_EXPIRY);
        assert!(r.token.is_empty());
        assert_eq!(r.kube.as_ref().unwrap().cert_pem, "CERT");
        let c: Copied = serde_json::from_str(&secret).unwrap();
        assert_eq!(c.target.key_pem, "CHAVE");
    }
}
