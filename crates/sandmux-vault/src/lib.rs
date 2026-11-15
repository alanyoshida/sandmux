//! Cofre de segredos do sandmux.
//!
//! Um arquivo JSON com um cabeçalho em claro (versão, parâmetros do Argon2id,
//! sal e nonce) e o conteúdo cifrado com XChaCha20-Poly1305, com o cabeçalho
//! como dado associado. A chave vem da senha pelo Argon2id. Nada do conteúdo
//! (nomes, valores, sessões) fica em claro.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// Versão do formato.
pub const VERSION: u32 = 1;

/// Um segredo. O valor some da memória quando o segredo é solto.
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Secret {
    pub name: String,
    pub value: String,
    /// Domínios donos (padrões de destino), para a troca no tráfego.
    #[serde(default)]
    pub domains: Vec<String>,
    /// Valor falso que a importação pôs no lugar, quando veio de lá.
    #[serde(default)]
    pub fake: Option<String>,
    /// Ferramentas que usam o segredo (`gh`, `git`, `npm`, `pip`, `cargo`,
    /// `docker`).
    #[serde(default)]
    pub tools: Vec<String>,
    /// Tipo: vazio para um token, [`KIND_SSH`] para uma chave SSH (o valor
    /// é a chave privada OpenSSH sem passphrase e os domínios são os
    /// servidores).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
}

/// Tipo de uma chave SSH.
pub const KIND_SSH: &str = "ssh";
/// Tipo de uma conexão de banco (o valor é um JSON com a senha).
pub const KIND_DB: &str = "db";

impl Secret {
    /// Uma chave SSH (fora da troca de tokens).
    pub fn is_ssh(&self) -> bool {
        self.kind == KIND_SSH
    }

    /// Um token (o tipo comum): só eles têm valor falso, ferramentas e
    /// troca no tráfego.
    pub fn is_token(&self) -> bool {
        self.kind.is_empty()
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secret")
            .field("name", &self.name)
            .field("domains", &self.domains)
            .finish_non_exhaustive()
    }
}

/// O conteúdo cifrado. Os valores somem da memória quando ele é solto.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Contents {
    #[serde(default)]
    pub secrets: Vec<Secret>,
    /// Valores de `--pass-env` por sessão.
    #[serde(default)]
    pub pass_env: BTreeMap<u32, BTreeMap<String, String>>,
    /// Chave do histórico de tela em disco (base64 de 32 bytes), criada no
    /// primeiro uso.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_key: Option<String>,
}

impl Drop for Contents {
    fn drop(&mut self) {
        for vars in self.pass_env.values_mut() {
            for v in vars.values_mut() {
                v.zeroize();
            }
        }
        if let Some(k) = self.history_key.as_mut() {
            k.zeroize();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Kdf {
    alg: String,
    m_kib: u32,
    t: u32,
    p: u32,
    salt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    kdf: Kdf,
    nonce: String,
    data: String,
}

/// Erros do cofre.
#[derive(Debug)]
pub enum VaultError {
    /// A senha não abre o cofre (ou o arquivo foi alterado).
    WrongPassword,
    /// O arquivo não é um cofre válido.
    Invalid(String),
    Io(io::Error),
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VaultError::WrongPassword => f.write_str(sandmux_spec::tr!(
                "a senha não confere (ou o arquivo do cofre foi alterado)",
                "the password does not match (or the vault file was changed)"
            )),
            VaultError::Invalid(m) => f.write_str(&sandmux_spec::trf!(
                "o arquivo do cofre é inválido: {m}",
                "the vault file is invalid: {m}"
            )),
            VaultError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for VaultError {}

impl From<io::Error> for VaultError {
    fn from(e: io::Error) -> Self {
        VaultError::Io(e)
    }
}

/// Onde o cofre fica. Um keyring do sistema pode entrar como outro `Store`.
pub trait Store: Send + Sync {
    fn load(&self) -> io::Result<Option<Vec<u8>>>;
    fn save(&self, bytes: &[u8]) -> io::Result<()>;
}

/// O cofre num arquivo, gravado de forma atômica com modo 600.
pub struct FileStore(pub PathBuf);

impl Store for FileStore {
    fn load(&self) -> io::Result<Option<Vec<u8>>> {
        match std::fs::read(&self.0) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn save(&self, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let dir = self.0.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!(
            ".{}.tmp",
            self.0.file_name().unwrap_or_default().to_string_lossy()
        ));
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, &self.0)
    }
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    let mut got = 0;
    while got < N {
        // SAFETY: escrita dentro do buffer, com o tamanho restante.
        let n = unsafe { libc::getrandom(b[got..].as_mut_ptr().cast(), N - got, 0) };
        if n > 0 {
            got += n as usize;
        }
    }
    b
}

/// Prefixo dos valores falsos (o mesmo da importação).
pub const FAKE_PREFIX: &str = "sandmux-fake-";

/// Um valor falso novo: o prefixo e 32 caracteres hexadecimais aleatórios.
pub fn new_fake() -> String {
    let b = random::<16>();
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{FAKE_PREFIX}{hex}")
}

/// Parâmetros do Argon2id: 64 MiB, 3 passadas, 1 via.
const M_KIB: u32 = 64 * 1024;
const T: u32 = 3;
const P: u32 = 1;

fn derive(password: &str, kdf: &Kdf) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    if kdf.alg != "argon2id" {
        return Err(VaultError::Invalid(sandmux_spec::trf!(
            "KDF desconhecida: {}",
            "unknown KDF: {}",
            kdf.alg
        )));
    }
    let salt = B64
        .decode(&kdf.salt)
        .map_err(|e| VaultError::Invalid(e.to_string()))?;
    let params = argon2::Params::new(kdf.m_kib, kdf.t, kdf.p, Some(32))
        .map_err(|e| VaultError::Invalid(e.to_string()))?;
    let a = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    a.hash_password_into(password.as_bytes(), &salt, key.as_mut())
        .map_err(|e| VaultError::Invalid(e.to_string()))?;
    Ok(key)
}

/// Dado associado: o cabeçalho, sem o conteúdo.
fn aad(kdf: &Kdf, nonce: &str) -> Vec<u8> {
    format!(
        "sandmux-vault:{VERSION}:{}:{}:{}:{}:{}:{nonce}",
        kdf.alg, kdf.m_kib, kdf.t, kdf.p, kdf.salt
    )
    .into_bytes()
}

/// O cofre aberto: a chave derivada e o conteúdo, só na memória.
pub struct Unlocked {
    key: Zeroizing<[u8; 32]>,
    kdf: Kdf,
    pub contents: Contents,
}

impl Unlocked {
    /// Cria um cofre novo com a senha.
    pub fn create(password: &str) -> Result<Unlocked, VaultError> {
        let kdf = Kdf {
            alg: "argon2id".into(),
            m_kib: M_KIB,
            t: T,
            p: P,
            salt: B64.encode(random::<16>()),
        };
        let key = derive(password, &kdf)?;
        Ok(Unlocked {
            key,
            kdf,
            contents: Contents::default(),
        })
    }

    /// Abre um cofre gravado.
    pub fn open(bytes: &[u8], password: &str) -> Result<Unlocked, VaultError> {
        let f: File =
            serde_json::from_slice(bytes).map_err(|e| VaultError::Invalid(e.to_string()))?;
        if f.version != VERSION {
            return Err(VaultError::Invalid(sandmux_spec::trf!(
                "versão {}",
                "version {}",
                f.version
            )));
        }
        let key = derive(password, &f.kdf)?;
        let nonce = B64
            .decode(&f.nonce)
            .map_err(|e| VaultError::Invalid(e.to_string()))?;
        if nonce.len() != 24 {
            return Err(VaultError::Invalid("nonce".into()));
        }
        let data = B64
            .decode(&f.data)
            .map_err(|e| VaultError::Invalid(e.to_string()))?;
        let cipher = XChaCha20Poly1305::new(key.as_ref().into());
        let plain = Zeroizing::new(
            cipher
                .decrypt(
                    XNonce::from_slice(&nonce),
                    Payload {
                        msg: &data,
                        aad: &aad(&f.kdf, &f.nonce),
                    },
                )
                .map_err(|_| VaultError::WrongPassword)?,
        );
        let contents: Contents =
            serde_json::from_slice(&plain).map_err(|e| VaultError::Invalid(e.to_string()))?;
        Ok(Unlocked {
            key,
            kdf: f.kdf,
            contents,
        })
    }

    /// O arquivo cifrado com o conteúdo atual (nonce novo a cada chamada).
    pub fn seal(&self) -> Result<Vec<u8>, VaultError> {
        let nonce_b = random::<24>();
        let nonce = B64.encode(nonce_b);
        let plain = Zeroizing::new(
            serde_json::to_vec(&self.contents).map_err(|e| VaultError::Invalid(e.to_string()))?,
        );
        let cipher = XChaCha20Poly1305::new(self.key.as_ref().into());
        let data = cipher
            .encrypt(
                XNonce::from_slice(&nonce_b),
                Payload {
                    msg: &plain,
                    aad: &aad(&self.kdf, &nonce),
                },
            )
            .map_err(|_| VaultError::Invalid(sandmux_spec::tr!("cifra", "cipher").into()))?;
        let f = File {
            version: VERSION,
            kdf: self.kdf.clone(),
            nonce,
            data: B64.encode(data),
        };
        serde_json::to_vec(&f).map_err(|e| VaultError::Invalid(e.to_string()))
    }

    /// Troca a senha: sal e chave novos.
    pub fn change_password(&mut self, new: &str) -> Result<(), VaultError> {
        let fresh = Unlocked::create(new)?;
        self.key = fresh.key.clone();
        self.kdf = fresh.kdf.clone();
        Ok(())
    }

    /// A chave do histórico de tela, criada se ainda não existe. Devolve a
    /// chave e se o conteúdo mudou (para gravar o cofre).
    pub fn history_key(&mut self) -> (Zeroizing<[u8; 32]>, bool) {
        if let Some(k) = &self.contents.history_key
            && let Ok(b) = B64.decode(k)
            && b.len() == 32
        {
            let mut out = Zeroizing::new([0u8; 32]);
            out.copy_from_slice(&b);
            return (out, false);
        }
        let k = Zeroizing::new(random::<32>());
        self.contents.history_key = Some(B64.encode(k.as_ref()));
        (k, true)
    }

    /// Dá um valor falso aos segredos que não têm. Devolve se mudou algo.
    pub fn fill_fakes(&mut self) -> bool {
        let mut changed = false;
        for s in &mut self.contents.secrets {
            if s.fake.is_none() && s.is_token() {
                s.fake = Some(new_fake());
                changed = true;
            }
        }
        changed
    }

    /// Acrescenta ou substitui um segredo pelo nome; sem valor falso, ganha um.
    pub fn put(&mut self, mut s: Secret) {
        if s.fake.is_none() && s.is_token() {
            s.fake = Some(new_fake());
        }
        self.contents.secrets.retain(|x| x.name != s.name);
        self.contents.secrets.push(s);
    }

    /// Tira um segredo; devolve se existia.
    pub fn remove(&mut self, name: &str) -> bool {
        let n = self.contents.secrets.len();
        self.contents.secrets.retain(|x| x.name != name);
        n != self.contents.secrets.len()
    }
}

/// Histórico de tela cifrado: registros `nonce(24) | tamanho(u32 LE) |
/// cifra`, com XChaCha20-Poly1305 e o AAD da aba.
pub mod history {
    use super::{Aead, KeyInit, Payload, VaultError, XChaCha20Poly1305, XNonce, random};

    fn aad(id: u32) -> Vec<u8> {
        format!("sandmux-history:{id}").into_bytes()
    }

    /// Um registro com o trecho `plain` da aba `id`.
    pub fn seal(key: &[u8; 32], id: u32, plain: &[u8]) -> Vec<u8> {
        let nonce = random::<24>();
        let cipher = XChaCha20Poly1305::new(key.into());
        let ct = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plain,
                    aad: &aad(id),
                },
            )
            .unwrap_or_default();
        let mut out = Vec::with_capacity(28 + ct.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&(ct.len() as u32).to_le_bytes());
        out.extend_from_slice(&ct);
        out
    }

    /// Os trechos do arquivo, em ordem. Um registro cortado no fim (queda no
    /// meio da gravação) é ignorado; um registro que não decifra (chave ou
    /// aba errada) é erro.
    pub fn open(key: &[u8; 32], id: u32, mut bytes: &[u8]) -> Result<Vec<u8>, VaultError> {
        let cipher = XChaCha20Poly1305::new(key.into());
        let mut out = Vec::new();
        while bytes.len() >= 28 {
            let len = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]) as usize;
            if bytes.len() < 28 + len {
                break;
            }
            let plain = cipher
                .decrypt(
                    XNonce::from_slice(&bytes[..24]),
                    Payload {
                        msg: &bytes[28..28 + len],
                        aad: &aad(id),
                    },
                )
                .map_err(|_| VaultError::WrongPassword)?;
            out.extend_from_slice(&plain);
            bytes = &bytes[28 + len..];
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(name: &str, value: &str) -> Secret {
        Secret {
            name: name.into(),
            value: value.into(),
            domains: vec!["openrouter.ai".into()],
            fake: None,
            kind: String::new(),
            tools: Vec::new(),
        }
    }

    #[test]
    fn ida_e_volta_e_senha_errada() {
        let mut v = Unlocked::create("senha boa").unwrap();
        v.put(secret("openrouter", "sk-teste-123"));
        v.contents
            .pass_env
            .entry(7)
            .or_default()
            .insert("CHAVE".into(), "valor-x".into());
        let bytes = v.seal().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        for leak in ["sk-teste-123", "openrouter", "CHAVE", "valor-x"] {
            assert!(!text.contains(leak), "{leak} em claro");
        }
        let o = Unlocked::open(&bytes, "senha boa").unwrap();
        assert_eq!(o.contents.secrets[0].value, "sk-teste-123");
        assert_eq!(o.contents.pass_env[&7]["CHAVE"], "valor-x");
        assert!(matches!(
            Unlocked::open(&bytes, "errada"),
            Err(VaultError::WrongPassword)
        ));
        // Duas gravações, nonces diferentes.
        assert_ne!(v.seal().unwrap(), bytes);
    }

    #[test]
    fn cabecalho_adulterado_nao_abre() {
        let v = Unlocked::create("s").unwrap();
        let bytes = v.seal().unwrap();
        let mut f: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        f["kdf"]["t"] = serde_json::json!(1);
        let changed = serde_json::to_vec(&f).unwrap();
        assert!(Unlocked::open(&changed, "s").is_err());
        assert!(matches!(
            Unlocked::open(b"{}", "s"),
            Err(VaultError::Invalid(_))
        ));
    }

    #[test]
    fn troca_de_senha_e_arquivo() {
        let d = tempfile::tempdir().unwrap();
        let st = FileStore(d.path().join("vault.json"));
        assert!(st.load().unwrap().is_none());
        let mut v = Unlocked::create("velha").unwrap();
        v.put(secret("a", "1"));
        v.change_password("nova").unwrap();
        st.save(&v.seal().unwrap()).unwrap();
        let bytes = st.load().unwrap().unwrap();
        assert!(Unlocked::open(&bytes, "velha").is_err());
        let mut o = Unlocked::open(&bytes, "nova").unwrap();
        assert!(o.remove("a") && !o.remove("a"));
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&st.0).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert!(!format!("{:?}", secret("x", "segredo")).contains("segredo"));
        // Valor falso gerado.
        o.put(secret("b", "2"));
        let f = o.contents.secrets[0].fake.clone().unwrap();
        assert!(
            f.starts_with(FAKE_PREFIX) && f.len() == FAKE_PREFIX.len() + 32,
            "{f}"
        );
        assert_ne!(new_fake(), new_fake());
    }

    #[test]
    fn historico_cifrado_ida_e_volta() {
        let k = random::<32>();
        let mut f = history::seal(&k, 3, b"linha um\r\n");
        f.extend(history::seal(&k, 3, b"linha dois\r\n"));
        assert!(!f.windows(8).any(|w| w == b"linha um"));
        assert_eq!(
            history::open(&k, 3, &f).unwrap(),
            b"linha um\r\nlinha dois\r\n"
        );
        // Registro cortado no fim: ignorado.
        let cut = &f[..f.len() - 5];
        assert_eq!(history::open(&k, 3, cut).unwrap(), b"linha um\r\n");
        // Chave ou aba errada: erro.
        assert!(history::open(&random::<32>(), 3, &f).is_err());
        assert!(history::open(&k, 4, &f).is_err());
    }

    #[test]
    fn chave_do_historico_criada_uma_vez() {
        let mut u = Unlocked::create("s").unwrap();
        let (a, changed) = u.history_key();
        assert!(changed);
        let (b, changed) = u.history_key();
        assert!(!changed);
        assert_eq!(*a, *b);
    }
}
