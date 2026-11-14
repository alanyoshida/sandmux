//! Agente SSH do sandmux: as chaves do cofre, o protocolo do agente
//! (subconjunto do `draft-miller-ssh-agent`: listar e assinar) e a leitura
//! das chaves no cadastro.

use ssh_key::{Algorithm, EcdsaCurve, HashAlg, PrivateKey, Signature};

const FAILURE: u8 = 5;
const REQUEST_IDENTITIES: u8 = 11;
const IDENTITIES_ANSWER: u8 = 12;
const SIGN_REQUEST: u8 = 13;
const SIGN_RESPONSE: u8 = 14;
const RSA_SHA2_256: u32 = 2;
const RSA_SHA2_512: u32 = 4;

/// Maior mensagem aceita do cliente.
pub const MAX_MSG: usize = 256 * 1024;

/// Lê uma chave privada OpenSSH sem passphrase de um tipo aceito.
pub fn parse_key(text: &str) -> Result<PrivateKey, String> {
    let k = PrivateKey::from_openssh(text).map_err(|e| format!("chave OpenSSH inválida: {e}"))?;
    if k.is_encrypted() {
        return Err("a chave ainda está cifrada".into());
    }
    check_algorithm(&k)?;
    Ok(k)
}

/// Lê uma chave privada OpenSSH, decifrando-a com `passphrase` quando é
/// cifrada, e a devolve sem passphrase, no formato OpenSSH.
pub fn import_key(text: &str, passphrase: Option<&str>) -> Result<String, String> {
    let k = PrivateKey::from_openssh(text).map_err(|e| format!("chave OpenSSH inválida: {e}"))?;
    let k = if k.is_encrypted() {
        let p = passphrase.ok_or("a chave é cifrada; falta a passphrase")?;
        k.decrypt(p).map_err(|_| "passphrase errada".to_string())?
    } else {
        k
    };
    check_algorithm(&k)?;
    k.to_openssh(ssh_key::LineEnding::LF)
        .map(|z| z.to_string())
        .map_err(|e| format!("gravar a chave: {e}"))
}

/// Se a chave lida de `text` é cifrada.
pub fn is_encrypted(text: &str) -> Result<bool, String> {
    PrivateKey::from_openssh(text)
        .map(|k| k.is_encrypted())
        .map_err(|e| format!("chave OpenSSH inválida: {e}"))
}

fn check_algorithm(k: &PrivateKey) -> Result<(), String> {
    match k.algorithm() {
        Algorithm::Ed25519
        | Algorithm::Rsa { .. }
        | Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256 | EcdsaCurve::NistP384,
        } => Ok(()),
        a => Err(format!(
            "tipo de chave não aceito: {}; use Ed25519, ECDSA (P-256, P-384) ou RSA",
            a.as_str()
        )),
    }
}

/// Impressão digital `SHA256:...` da chave.
pub fn fingerprint(k: &PrivateKey) -> String {
    k.public_key().fingerprint(HashAlg::Sha256).to_string()
}

/// Uma chave que o agente pode usar.
pub struct AgentKey {
    pub name: String,
    pub key: PrivateKey,
}

/// O que aconteceu num pedido de assinatura, para o registro.
#[derive(Debug, PartialEq, Eq)]
pub enum SignEvent {
    Signed { name: String, fingerprint: String },
    Refused { reason: String },
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn put_string(out: &mut Vec<u8>, b: &[u8]) {
    put_u32(out, b.len() as u32);
    out.extend_from_slice(b);
}

fn get_u32(b: &mut &[u8]) -> Option<u32> {
    let (h, t) = b.split_at_checked(4)?;
    *b = t;
    Some(u32::from_be_bytes(h.try_into().ok()?))
}

fn get_string<'a>(b: &mut &'a [u8]) -> Option<&'a [u8]> {
    let n = get_u32(b)? as usize;
    let (h, t) = b.split_at_checked(n)?;
    *b = t;
    Some(h)
}

/// A chave RSA do `rsa`, montada aqui: a conversão do `ssh-key` 0.6.7 passa
/// o primo `p` duas vezes e falha.
fn rsa_key(k: &ssh_key::private::RsaKeypair) -> Result<rsa::RsaPrivateKey, String> {
    let n = |m: &ssh_key::Mpint| {
        m.as_positive_bytes()
            .map(rsa::BigUint::from_bytes_be)
            .ok_or_else(|| "chave RSA inválida".to_string())
    };
    rsa::RsaPrivateKey::from_components(
        n(&k.public.n)?,
        n(&k.public.e)?,
        n(&k.private.d)?,
        vec![n(&k.private.p)?, n(&k.private.q)?],
    )
    .map_err(|e| format!("chave RSA inválida: {e}"))
}

fn sign(k: &PrivateKey, data: &[u8], flags: u32) -> Result<Signature, String> {
    use signature::Signer;
    match k.key_data() {
        ssh_key::private::KeypairData::Rsa(rsa) => {
            let key = rsa_key(rsa)?;
            let (hash, bytes) = if flags & RSA_SHA2_512 != 0 {
                let sk = rsa::pkcs1v15::SigningKey::<ssh_key::sha2::Sha512>::new(key);
                (
                    HashAlg::Sha512,
                    signature::SignatureEncoding::to_vec(
                        &sk.try_sign(data).map_err(|e| e.to_string())?,
                    ),
                )
            } else if flags & RSA_SHA2_256 != 0 {
                let sk = rsa::pkcs1v15::SigningKey::<ssh_key::sha2::Sha256>::new(key);
                (
                    HashAlg::Sha256,
                    signature::SignatureEncoding::to_vec(
                        &sk.try_sign(data).map_err(|e| e.to_string())?,
                    ),
                )
            } else {
                return Err("RSA com SHA-1 (ssh-rsa) recusado".into());
            };
            Signature::new(Algorithm::Rsa { hash: Some(hash) }, bytes).map_err(|e| e.to_string())
        }
        _ => k.try_sign(data).map_err(|e| e.to_string()),
    }
}

/// Atende um pedido (`msg`, sem o tamanho) com as chaves `keys` (as
/// escolhidas na sessão e presentes no cofre aberto). Devolve a resposta (sem
/// o tamanho) e, num pedido de assinatura, o evento para o registro.
pub fn handle(msg: &[u8], keys: &[AgentKey]) -> (Vec<u8>, Option<SignEvent>) {
    let fail = vec![FAILURE];
    let Some((&kind, mut body)) = msg.split_first() else {
        return (fail, None);
    };
    match kind {
        REQUEST_IDENTITIES => {
            let mut out = vec![IDENTITIES_ANSWER];
            let mut n = 0u32;
            let mut list = Vec::new();
            for k in keys {
                let Ok(blob) = k.key.public_key().to_bytes() else {
                    continue;
                };
                put_string(&mut list, &blob);
                put_string(&mut list, k.name.as_bytes());
                n += 1;
            }
            put_u32(&mut out, n);
            out.extend_from_slice(&list);
            (out, None)
        }
        SIGN_REQUEST => {
            let parsed = (|| {
                let blob = get_string(&mut body)?;
                let data = get_string(&mut body)?;
                let flags = get_u32(&mut body)?;
                Some((blob, data, flags))
            })();
            let Some((blob, data, flags)) = parsed else {
                return (
                    fail,
                    Some(SignEvent::Refused {
                        reason: "pedido malformado".into(),
                    }),
                );
            };
            let Some(k) = keys.iter().find(|k| {
                k.key
                    .public_key()
                    .to_bytes()
                    .is_ok_and(|b| b.as_slice() == blob)
            }) else {
                return (
                    fail,
                    Some(SignEvent::Refused {
                        reason: "chave não escolhida ou cofre trancado".into(),
                    }),
                );
            };
            let sig = sign(&k.key, data, flags)
                .and_then(|s| Vec::<u8>::try_from(s).map_err(|e| e.to_string()));
            match sig {
                Ok(sig) => {
                    let mut out = vec![SIGN_RESPONSE];
                    put_string(&mut out, &sig);
                    (
                        out,
                        Some(SignEvent::Signed {
                            name: k.name.clone(),
                            fingerprint: fingerprint(&k.key),
                        }),
                    )
                }
                Err(reason) => (fail, Some(SignEvent::Refused { reason })),
            }
        }
        _ => (fail, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signature::Verifier;

    fn keygen(kind: &str, dir: &std::path::Path, pass: &str) -> String {
        let f = dir.join(kind);
        let mut args = vec!["-q", "-t", kind, "-N", pass, "-C", "teste", "-f"];
        let fs = f.to_str().unwrap().to_string();
        args.push(&fs);
        if kind == "rsa" {
            args.extend(["-b", "2048"]);
        }
        let ok = std::process::Command::new("ssh-keygen")
            .args(&args)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        std::fs::read_to_string(f).unwrap()
    }

    fn request(blob: &[u8], data: &[u8], flags: u32) -> Vec<u8> {
        let mut m = vec![SIGN_REQUEST];
        put_string(&mut m, blob);
        put_string(&mut m, data);
        put_u32(&mut m, flags);
        m
    }

    fn signature_of(resp: &[u8]) -> Signature {
        assert_eq!(resp[0], SIGN_RESPONSE);
        let mut b = &resp[1..];
        let s = get_string(&mut b).unwrap();
        Signature::try_from(s).unwrap()
    }

    #[test]
    fn lista_e_assina_com_cada_tipo() {
        let d = tempfile::tempdir().unwrap();
        for (kind, flags) in [
            ("ed25519", 0),
            ("ecdsa", 0),
            ("rsa", RSA_SHA2_256),
            ("rsa", RSA_SHA2_512),
        ] {
            let text = import_key(&keygen(kind, d.path(), ""), None).unwrap();
            let _ = std::fs::remove_file(d.path().join(kind));
            let _ = std::fs::remove_file(d.path().join(format!("{kind}.pub")));
            let key = parse_key(&text).unwrap();
            let keys = vec![AgentKey {
                name: "github".into(),
                key,
            }];
            let (resp, ev) = handle(&[REQUEST_IDENTITIES], &keys);
            assert_eq!(ev, None);
            assert_eq!(resp[0], IDENTITIES_ANSWER);
            let mut b = &resp[1..];
            assert_eq!(get_u32(&mut b), Some(1));
            let blob = get_string(&mut b).unwrap().to_vec();
            assert_eq!(get_string(&mut b), Some(&b"github"[..]));

            let (resp, ev) = handle(&request(&blob, b"dados", flags), &keys);
            assert_eq!(resp[0], SIGN_RESPONSE, "{kind}: {ev:?}");
            let sig = signature_of(&resp);
            Verifier::verify(keys[0].key.public_key().key_data(), b"dados", &sig)
                .unwrap_or_else(|e| panic!("{kind}: {e}"));
            assert!(matches!(ev, Some(SignEvent::Signed { ref name, .. }) if name == "github"));
            if kind == "rsa" {
                // SHA-1 recusado.
                let (resp, ev) = handle(&request(&blob, b"dados", 0), &keys);
                assert_eq!(resp, vec![FAILURE]);
                assert!(matches!(ev, Some(SignEvent::Refused { .. })));
            }
        }
    }

    #[test]
    fn recusa_chave_de_fora_e_outros_pedidos() {
        let d = tempfile::tempdir().unwrap();
        let a = parse_key(&import_key(&keygen("ed25519", d.path(), ""), None).unwrap()).unwrap();
        let other_blob = a.public_key().to_bytes().unwrap();
        // Sem chaves (cofre trancado): lista vazia e assinatura recusada.
        let (resp, _) = handle(&[REQUEST_IDENTITIES], &[]);
        assert_eq!(resp, vec![IDENTITIES_ANSWER, 0, 0, 0, 0]);
        let (resp, ev) = handle(&request(&other_blob, b"x", 0), &[]);
        assert_eq!(resp, vec![FAILURE]);
        assert!(matches!(ev, Some(SignEvent::Refused { .. })));
        // Adicionar (17), remover tudo (19), trancar (22), extensão (27).
        for k in [17u8, 19, 22, 27] {
            assert_eq!(handle(&[k, 0, 0, 0, 0], &[]).0, vec![FAILURE]);
        }
        assert_eq!(handle(&[], &[]).0, vec![FAILURE]);
    }

    #[test]
    fn importa_com_passphrase() {
        let d = tempfile::tempdir().unwrap();
        let text = keygen("ed25519", d.path(), "segredo");
        assert!(is_encrypted(&text).unwrap());
        assert_eq!(
            import_key(&text, Some("errada")).unwrap_err(),
            "passphrase errada"
        );
        assert!(import_key(&text, None).is_err());
        let plain = import_key(&text, Some("segredo")).unwrap();
        let k = parse_key(&plain).unwrap();
        assert!(fingerprint(&k).starts_with("SHA256:"));
        assert!(parse_key(&text).is_err(), "a cifrada não serve ao agente");
    }
}
