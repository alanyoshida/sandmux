//! TLS do gateway: a CA de cada lançamento, os certificados por domínio
//! assinados por ela e as configurações do `rustls` dos dois lados.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, KeyUsagePurpose,
    PKCS_ECDSA_P256_SHA256,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Gera uma CA nova: (certificado PEM, chave PEM). Válida por 30 dias.
pub fn generate_ca() -> Result<(String, String), String> {
    let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).map_err(|e| e.to_string())?;
    let mut p = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "sandmux CA do lançamento");
    dn.push(DnType::OrganizationName, "sandmux");
    p.distinguished_name = dn;
    p.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    p.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let now = std::time::SystemTime::now();
    p.not_before = (now - std::time::Duration::from_secs(3600)).into();
    p.not_after = (now + std::time::Duration::from_secs(30 * 86_400)).into();
    let cert = p.self_signed(&key).map_err(|e| e.to_string())?;
    Ok((cert.pem(), key.serialize_pem()))
}

/// A CA de um lançamento, que assina certificados por domínio, com cache.
pub struct Ca {
    cert: rcgen::Certificate,
    key: KeyPair,
    cache: Mutex<HashMap<(String, bool), Arc<rustls::ServerConfig>>>,
}

impl Ca {
    pub fn from_pem(cert_pem: &str, key_pem: &str) -> Result<Ca, String> {
        let key = KeyPair::from_pem(key_pem).map_err(|e| e.to_string())?;
        let params = CertificateParams::from_ca_cert_pem(cert_pem).map_err(|e| e.to_string())?;
        // Reassina com a mesma chave: o mesmo nome e a mesma chave pública,
        // que é o que um emissor precisa.
        let cert = params.self_signed(&key).map_err(|e| e.to_string())?;
        Ok(Ca {
            cert,
            key,
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// Configuração do servidor TLS para `host`: certificado do domínio
    /// assinado pela CA, só HTTP/1.1.
    pub fn server_config(&self, host: &str) -> Result<Arc<rustls::ServerConfig>, String> {
        self.server_config_alpn(host, false)
    }

    /// Como [`Ca::server_config`], oferecendo também `h2` quando `h2`.
    pub fn server_config_alpn(
        &self,
        host: &str,
        h2: bool,
    ) -> Result<Arc<rustls::ServerConfig>, String> {
        let host = host.to_ascii_lowercase();
        if let Some(c) = self
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(host.clone(), h2))
        {
            return Ok(c.clone());
        }
        let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).map_err(|e| e.to_string())?;
        let mut p = CertificateParams::new(vec![host.clone()]).map_err(|e| e.to_string())?;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, host.clone());
        p.distinguished_name = dn;
        let now = std::time::SystemTime::now();
        p.not_before = (now - std::time::Duration::from_secs(3600)).into();
        p.not_after = (now + std::time::Duration::from_secs(7 * 86_400)).into();
        // O Python 3.13+ verifica em modo estrito (`VERIFY_X509_STRICT`) e
        // recusa um certificado sem o Authority Key Identifier.
        p.use_authority_key_identifier_extension = true;
        p.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        p.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        let leaf = p
            .signed_by(&key, &self.cert, &self.key)
            .map_err(|e| e.to_string())?;
        let chain = vec![leaf.der().clone(), self.cert.der().clone()];
        let pk = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
        let mut cfg = rustls::ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_no_client_auth()
            .with_single_cert(chain, pk)
            .map_err(|e| e.to_string())?;
        cfg.alpn_protocols = alpn(h2);
        let cfg = Arc::new(cfg);
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if g.len() > 1000 {
            g.clear();
        }
        g.insert((host, h2), cfg.clone());
        Ok(cfg)
    }
}

/// Raízes para os destinos: as do sistema, mais as de `extra_pem` (uma CA
/// adicional, como a de um servidor de desenvolvimento local).
fn roots(extra_pem: Option<&str>) -> rustls::RootCertStore {
    let mut r = rustls::RootCertStore::empty();
    for c in rustls_native_certs::load_native_certs().certs {
        let _ = r.add(c);
    }
    if let Some(pem) = extra_pem {
        for c in rustls_pemfile::certs(&mut pem.as_bytes()).flatten() {
            let _ = r.add(c);
        }
    }
    r
}

/// O ALPN oferecido: `h2` antes de `http/1.1` quando `h2`.
pub fn alpn(h2: bool) -> Vec<Vec<u8>> {
    if h2 {
        vec![b"h2".to_vec(), b"http/1.1".to_vec()]
    } else {
        vec![b"http/1.1".to_vec()]
    }
}

/// O protocolo que uma conexão TLS negociou é o HTTP/2.
pub fn negotiated_h2(alpn: Option<&[u8]>) -> bool {
    alpn == Some(b"h2".as_slice())
}

/// Configuração do cliente TLS até os destinos, só HTTP/1.1.
pub fn client_config(extra_pem: Option<&str>) -> Arc<rustls::ClientConfig> {
    client_config_alpn(extra_pem, false)
}

/// Configuração do cliente TLS até os destinos, oferecendo também `h2`
/// quando `h2`.
pub fn client_config_alpn(extra_pem: Option<&str>, h2: bool) -> Arc<rustls::ClientConfig> {
    static PLAIN: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    static PLAIN_H2: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    let build = |extra: Option<&str>| {
        let mut c = rustls::ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("versões do TLS")
            .with_root_certificates(roots(extra))
            .with_no_client_auth();
        c.alpn_protocols = alpn(h2);
        Arc::new(c)
    };
    match (extra_pem, h2) {
        (None, false) => PLAIN.get_or_init(|| build(None)).clone(),
        (None, true) => PLAIN_H2.get_or_init(|| build(None)).clone(),
        (Some(_), _) => build(extra_pem),
    }
}

/// O cliente TLS até o API server de uma identidade de Kubernetes: a CA do
/// cluster (ou as raízes de sempre, sem ela), o certificado de cliente
/// quando há, e nenhuma verificação só com `insecure`. Só HTTP/1.1.
pub fn kube_client_config(
    t: &sandmux_proto::msg::KubeTarget,
    extra_pem: Option<&str>,
    h2: bool,
) -> Result<Arc<rustls::ClientConfig>, String> {
    let builder = rustls::ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?;
    let builder = if t.insecure {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerify(provider())))
    } else if t.ca_pem.trim().is_empty() {
        builder.with_root_certificates(roots(extra_pem))
    } else {
        let mut r = rustls::RootCertStore::empty();
        for c in certs_of(&t.ca_pem) {
            r.add(c).map_err(|e| e.to_string())?;
        }
        if r.is_empty() {
            return Err(sandmux_spec::tr!(
                "a CA do cluster não tem certificado",
                "the cluster CA has no certificate"
            )
            .into());
        }
        builder.with_root_certificates(r)
    };
    let mut c = if t.cert_pem.is_empty() {
        builder.with_no_client_auth()
    } else {
        let chain = certs_of(&t.cert_pem);
        let key = rustls_pemfile::private_key(&mut t.key_pem.as_bytes())
            .ok()
            .flatten()
            .ok_or_else(|| {
                sandmux_spec::tr!(
                    "a chave do certificado de cliente é inválida",
                    "the client certificate key is invalid"
                )
                .to_string()
            })?;
        builder
            .with_client_auth_cert(chain, key)
            .map_err(|e| e.to_string())?
    };
    c.alpn_protocols = alpn(h2);
    Ok(Arc::new(c))
}

/// Aceita qualquer certificado do destino (o `insecure-skip-tls-verify` do
/// kubeconfig), conferindo só as assinaturas do handshake.
#[derive(Debug)]
struct NoVerify(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Os certificados de um PEM, para quem confia na CA (testes).
pub fn certs_of(pem: &str) -> Vec<CertificateDer<'static>> {
    rustls_pemfile::certs(&mut pem.as_bytes())
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::pki_types::ServerName;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn certificado_do_dominio_confiavel_pela_ca() {
        let (cert, key) = generate_ca().unwrap();
        assert!(cert.contains("BEGIN CERTIFICATE") && key.contains("PRIVATE KEY"));
        let ca = Ca::from_pem(&cert, &key).unwrap();
        let server = ca.server_config("api.exemplo.test").unwrap();
        // Do cache, o mesmo.
        assert!(Arc::ptr_eq(
            &server,
            &ca.server_config("API.exemplo.test").unwrap()
        ));

        let mut roots = rustls::RootCertStore::empty();
        for c in certs_of(&cert) {
            roots.add(c).unwrap();
        }
        let client = Arc::new(
            rustls::ClientConfig::builder_with_provider(provider())
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        let (a, b) = tokio::io::duplex(64 * 1024);
        let acceptor = tokio_rustls::TlsAcceptor::from(server);
        let srv = tokio::spawn(async move {
            let mut s = acceptor.accept(a).await.unwrap();
            let mut buf = [0u8; 3];
            s.read_exact(&mut buf).await.unwrap();
            s.write_all(&buf).await.unwrap();
            s.flush().await.unwrap();
        });
        let conn = tokio_rustls::TlsConnector::from(client);
        let mut c = conn
            .connect(ServerName::try_from("api.exemplo.test").unwrap(), b)
            .await
            .unwrap();
        c.write_all(b"ola").await.unwrap();
        let mut back = [0u8; 3];
        c.read_exact(&mut back).await.unwrap();
        assert_eq!(&back, b"ola");
        srv.await.unwrap();

        // Outra CA não serve.
        let (cert2, _) = generate_ca().unwrap();
        assert_ne!(cert, cert2);
    }
}
