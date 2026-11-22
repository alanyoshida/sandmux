//! As identidades da AWS: os perfis do host lidos dos arquivos dele, o real
//! tirado do `aws` do host (`aws configure export-credentials`) ou da cópia
//! no cofre, e a chave real que só o gateway recebe.

use std::path::PathBuf;
use std::time::Duration;

use sandmux_proto::msg::{AwsCreds, CloudAdd};
use sandmux_spec::{tr, trf};

use crate::cloud::{GcloudErr, Real, now, run_host};

/// Troca o `aws` do host (testes).
pub const AWS_ENV: &str = "SANDMUX_AWS";
const AWS_TIMEOUT: Duration = Duration::from_secs(60);

pub fn aws_bin() -> String {
    std::env::var(AWS_ENV)
        .ok()
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "aws".into())
}

/// O `aws` está no host?
pub fn aws_present() -> bool {
    let bin = aws_bin();
    if bin.contains('/') {
        return std::path::Path::new(&bin).is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(&bin).is_file()))
}

fn home_file(var: &str, rel: &str) -> PathBuf {
    if let Some(p) = std::env::var_os(var).filter(|p| !p.is_empty()) {
        return p.into();
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(rel)
}

/// O `config` e o `credentials` do host.
pub fn host_files() -> (PathBuf, PathBuf) {
    (
        home_file("AWS_CONFIG_FILE", ".aws/config"),
        home_file("AWS_SHARED_CREDENTIALS_FILE", ".aws/credentials"),
    )
}

/// As variáveis dos arquivos do host, quando definidas: a referência roda o
/// `aws` com elas.
pub fn host_env() -> Vec<(String, String)> {
    ["AWS_CONFIG_FILE", "AWS_SHARED_CREDENTIALS_FILE"]
        .iter()
        .filter_map(|k| {
            std::env::var(k)
                .ok()
                .filter(|v| !v.is_empty())
                .map(|v| (k.to_string(), v))
        })
        .collect()
}

/// As seções de um INI, na ordem, com os pares de cada uma.
pub fn parse_ini(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with(';') {
            continue;
        }
        if let Some(name) = l.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
            out.push((name.trim().to_string(), Vec::new()));
        } else if let (Some((k, v)), Some(last)) = (l.split_once('='), out.last_mut()) {
            // Uma linha recuada continua o valor anterior (as subseções do
            // `s3 =`): fica de fora.
            if !line.starts_with([' ', '\t']) {
                last.1.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }
    out
}

/// Um perfil do host, como o comando o vê.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub region: String,
    /// A chave estática (ID e chave secreta), quando há.
    pub key: Option<(String, String)>,
    pub session_token: bool,
    /// SSO, `role_arn` ou `credential_process`: o real só vem do host.
    pub dynamic: bool,
}

/// Os perfis do `config` e do `credentials` juntos, na ordem do `config`.
pub fn profiles(config: &str, credentials: &str) -> Vec<Profile> {
    let mut list: Vec<Profile> = Vec::new();
    let mut entry = |name: &str| -> usize {
        match list.iter().position(|p| p.name == name) {
            Some(i) => i,
            None => {
                list.push(Profile {
                    name: name.to_string(),
                    ..Default::default()
                });
                list.len() - 1
            }
        }
    };
    let apply = |list_i: usize, pairs: &[(String, String)], list: &mut Vec<Profile>| {
        let p = &mut list[list_i];
        let get = |k: &str| pairs.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
        if let Some(r) = get("region") {
            p.region = r;
        }
        if let (Some(id), Some(secret)) = (get("aws_access_key_id"), get("aws_secret_access_key")) {
            p.key = Some((id, secret));
        }
        p.session_token |= get("aws_session_token").is_some();
        p.dynamic |= [
            "sso_session",
            "sso_start_url",
            "role_arn",
            "credential_process",
        ]
        .iter()
        .any(|k| get(k).is_some());
    };
    let mut pending = Vec::new();
    for (sec, pairs) in parse_ini(config) {
        let name = if sec == "default" {
            Some("default".to_string())
        } else {
            sec.strip_prefix("profile ").map(|n| n.trim().to_string())
        };
        if let Some(n) = name {
            pending.push((entry(&n), pairs));
        }
    }
    for (sec, pairs) in parse_ini(credentials) {
        pending.push((entry(&sec), pairs));
    }
    for (i, pairs) in pending {
        apply(i, &pairs, &mut list);
    }
    list
}

/// A chave de um perfil para a cópia, ou por que não dá.
pub fn copy_key(p: &Profile) -> Result<(String, String), String> {
    match &p.key {
        Some(k) if !p.session_token && !p.dynamic => Ok(k.clone()),
        Some(_) if p.session_token => Err(trf!(
            "o perfil {} tem um token de sessão, que vence: só uma chave estática pode ser copiada; guarde a referência (sem --copy)",
            "profile {} has a session token, which expires: only a static key can be copied; store the reference (without --copy)",
            p.name
        )),
        _ => Err(trf!(
            "o perfil {} não tem uma chave estática (SSO, role_arn ou credential_process): só uma chave estática pode ser copiada; guarde a referência (sem --copy)",
            "profile {} has no static key (SSO, role_arn or credential_process): only a static key can be copied; store the reference (without --copy)",
            p.name
        )),
    }
}

/// O real da saída do `aws configure export-credentials --format process`.
pub fn parse_export(out: &str, now: u64) -> Result<Real, String> {
    let v: serde_json::Value = serde_json::from_str(out).map_err(|e| {
        trf!(
            "a saída do aws não é um JSON válido: {e}",
            "the aws output is not valid JSON: {e}"
        )
    })?;
    let s = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let (id, secret) = (s("AccessKeyId"), s("SecretAccessKey"));
    if id.is_empty() || secret.is_empty() {
        return Err(tr!(
            "o aws do host não devolveu a chave",
            "the host aws did not return the key"
        )
        .into());
    }
    Ok(Real {
        token: String::new(),
        expires: sandmux_spec::net::parse_rfc3339(&s("Expiration"))
            .unwrap_or(now + crate::kube::NO_EXPIRY),
        kube: None,
        aws: Some(AwsCreds {
            access_key_id: id,
            secret,
            session_token: s("SessionToken"),
            session_fake: String::new(),
        }),
    })
}

/// O motivo de uma falha do `aws` do host, com o que fazer.
pub(crate) fn aws_failure(profile: &str, e: &GcloudErr) -> String {
    match e {
        GcloudErr::Missing => trf!(
            "o aws não está instalado no host; instale o AWS CLI v2 e rode `aws configure` no host (perfil {profile})",
            "aws is not installed on the host; install the AWS CLI v2 and run `aws configure` on the host (profile {profile})"
        ),
        GcloudErr::Failed(m) => {
            let last = m
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim();
            let low = m.to_ascii_lowercase();
            if low.contains("sso") && (low.contains("expired") || low.contains("login")) {
                return trf!(
                    "a sessão de SSO do perfil {profile} venceu: rode `aws sso login --profile {profile}` no host",
                    "the SSO session of profile {profile} expired: run `aws sso login --profile {profile}` on the host"
                );
            }
            trf!(
                "o aws do host falhou no perfil {profile}: {last}",
                "the host aws failed on profile {profile}: {last}"
            )
        }
    }
}

/// Obtém o real de uma identidade da AWS, fora do sandbox.
pub fn resolve(id: &CloudAdd) -> Result<Real, String> {
    match id.info.source.as_str() {
        "host" => {
            let p = id.info.configuration.as_str();
            let mut env = id.info.env.clone();
            env.push(("AWS_PAGER".into(), String::new()));
            let out = run_host(
                &aws_bin(),
                &[
                    "configure",
                    "export-credentials",
                    "--profile",
                    p,
                    "--format",
                    "process",
                ],
                &env,
                AWS_TIMEOUT,
            )
            .map_err(|e| aws_failure(p, &e))?;
            parse_export(&out, now())
        }
        "copy" => {
            let c: AwsCreds = serde_json::from_str(&id.secret).map_err(|e| e.to_string())?;
            Ok(Real {
                token: String::new(),
                expires: now() + crate::kube::NO_EXPIRY,
                kube: None,
                aws: Some(c),
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

    const CONFIG: &str = "[default]\nregion = us-east-1\n\n[profile Dev_SSO]\nsso_session = corp\nsso_account_id = 1\nregion = sa-east-1\n\n[profile papel]\nrole_arn = arn:aws:iam::1:role/x\nsource_profile = default\n\n[profile s3conf]\nregion = eu-west-1\ns3 =\n  addressing_style = path\n\n[sso-session corp]\nsso_start_url = https://x\n";
    const CREDS: &str = "[default]\naws_access_key_id = AKIAREAL\naws_secret_access_key = SEGREDO\n\n[temp]\naws_access_key_id = ASIAX\naws_secret_access_key = S\naws_session_token = T\n";

    #[test]
    fn perfis_do_host() {
        let p = profiles(CONFIG, CREDS);
        let names: Vec<&str> = p.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["default", "Dev_SSO", "papel", "s3conf", "temp"]);
        assert_eq!(p[0].region, "us-east-1");
        assert_eq!(
            copy_key(&p[0]).unwrap(),
            ("AKIAREAL".into(), "SEGREDO".into())
        );
        assert!(p[1].dynamic && p[1].region == "sa-east-1");
        assert!(copy_key(&p[1]).unwrap_err().contains("SSO"));
        assert!(copy_key(&p[2]).is_err());
        assert_eq!(p[3].region, "eu-west-1");
        assert!(copy_key(&p[4]).unwrap_err().contains("token de sessão"));
    }

    #[test]
    fn saida_do_export_credentials() {
        let r = parse_export(
            r#"{"Version":1,"AccessKeyId":"ASIAREAL","SecretAccessKey":"S","SessionToken":"T","Expiration":"2026-11-22T18:00:00+00:00"}"#,
            0,
        )
        .unwrap();
        let a = r.aws.unwrap();
        assert_eq!(
            (a.access_key_id.as_str(), a.session_token.as_str()),
            ("ASIAREAL", "T")
        );
        assert_eq!(
            Some(r.expires),
            sandmux_spec::net::parse_rfc3339("2026-11-22T18:00:00Z")
        );
        let r = parse_export(
            r#"{"Version":1,"AccessKeyId":"AKIA","SecretAccessKey":"S"}"#,
            100,
        )
        .unwrap();
        assert_eq!(r.expires, 100 + crate::kube::NO_EXPIRY);
        assert!(parse_export("{}", 0).is_err());
        let why = aws_failure(
            "dev",
            &GcloudErr::Failed(
                "\nError when retrieving token from sso: Token has expired and refresh failed\n"
                    .into(),
            ),
        );
        assert!(why.contains("aws sso login --profile dev"), "{why}");
        let why = aws_failure(
            "dev",
            &GcloudErr::Failed("a\nThe config profile (dev) could not be found\n".into()),
        );
        assert!(why.contains("could not be found"), "{why}");
    }
}
