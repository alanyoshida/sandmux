//! A assinatura SigV4 da AWS, refeita pelo gateway com a chave real sobre o
//! mesmo pedido canônico que o SDK da aba assinou com o par falso.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

/// O algoritmo no `Authorization` e na URL pré-assinada.
pub const ALGORITHM: &str = "AWS4-HMAC-SHA256";
/// O hash do corpo vazio.
pub const EMPTY_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
/// Os valores de `x-amz-content-sha256` dos uploads em pedaços assinados.
pub const STREAMING: &str = "STREAMING-AWS4-HMAC-SHA256-PAYLOAD";
pub const STREAMING_TRAILER: &str = "STREAMING-AWS4-HMAC-SHA256-PAYLOAD-TRAILER";

pub fn sha256_hex(b: &[u8]) -> String {
    hex(&Sha256::digest(b))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn hmac(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut m = Hmac::<Sha256>::new_from_slice(key).expect("HMAC aceita qualquer chave");
    m.update(msg);
    m.finalize().into_bytes().to_vec()
}

/// O escopo de uma credencial: data, região e serviço.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub date: String,
    pub region: String,
    pub service: String,
}

impl Scope {
    /// `data/região/serviço/aws4_request`.
    pub fn text(&self) -> String {
        format!(
            "{}/{}/{}/aws4_request",
            self.date, self.region, self.service
        )
    }

    /// A chave derivada da chave secreta, para este escopo.
    pub fn key(&self, secret: &str) -> Vec<u8> {
        let k = hmac(format!("AWS4{secret}").as_bytes(), self.date.as_bytes());
        let k = hmac(&k, self.region.as_bytes());
        let k = hmac(&k, self.service.as_bytes());
        hmac(&k, b"aws4_request")
    }
}

/// `ID/data/região/serviço/aws4_request` em ID e escopo.
pub fn parse_credential(c: &str) -> Option<(String, Scope)> {
    let mut p = c.split('/');
    let id = p.next()?.to_string();
    let scope = Scope {
        date: p.next()?.to_string(),
        region: p.next()?.to_string(),
        service: p.next()?.to_string(),
    };
    (p.next()? == "aws4_request" && p.next().is_none() && !id.is_empty()).then_some((id, scope))
}

/// O `Authorization` de um pedido assinado com SigV4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Auth {
    pub access_key: String,
    pub scope: Scope,
    pub signed: Vec<String>,
    pub signature: String,
}

impl Auth {
    pub fn parse(h: &str) -> Option<Auth> {
        let rest = h.trim().strip_prefix(ALGORITHM)?.trim_start();
        let (mut cred, mut signed, mut sig) = (None, None, None);
        for part in rest.split(',') {
            let (k, v) = part.trim().split_once('=')?;
            match k.trim() {
                "Credential" => cred = Some(v.trim()),
                "SignedHeaders" => signed = Some(v.trim()),
                "Signature" => sig = Some(v.trim()),
                _ => {}
            }
        }
        let (access_key, scope) = parse_credential(cred?)?;
        Some(Auth {
            access_key,
            scope,
            signed: signed?.split(';').map(|s| s.to_ascii_lowercase()).collect(),
            signature: sig?.to_string(),
        })
    }

    /// O cabeçalho, como o SDK o escreve.
    pub fn header(&self) -> String {
        format!(
            "{ALGORITHM} Credential={}/{}, SignedHeaders={}, Signature={}",
            self.access_key,
            self.scope.text(),
            self.signed.join(";"),
            self.signature
        )
    }
}

/// Codifica pela regra da AWS: só `A-Za-z0-9-_.~` sem escape, e a `/` só
/// quando `slash`.
pub fn uri_encode(b: &[u8], slash: bool) -> String {
    let mut out = String::with_capacity(b.len());
    for &c in b {
        if c.is_ascii_alphanumeric()
            || matches!(c, b'-' | b'_' | b'.' | b'~')
            || (c == b'/' && !slash)
        {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{c:02X}"));
        }
    }
    out
}

/// Desfaz os `%XX` (um `%` sem dois dígitos fica).
pub fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = std::str::from_utf8(&b[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or(())
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// O caminho canônico: no S3, como veio; nos outros serviços, cada
/// segmento (já codificado no pedido) codificado de novo.
pub fn canonical_uri(path: &str, s3: bool) -> String {
    let path = if path.is_empty() { "/" } else { path };
    if s3 {
        return path.to_string();
    }
    path.split('/')
        .map(|seg| uri_encode(seg.as_bytes(), true))
        .collect::<Vec<_>>()
        .join("/")
}

/// Os pares de uma query crua, na ordem e na codificação do pedido.
pub fn query_pairs(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (k.to_string(), v.to_string()),
            None => (p.to_string(), String::new()),
        })
        .collect()
}

/// A query canônica: cada par decodificado e codificado de novo, em ordem.
/// `skip` tira um parâmetro (a `X-Amz-Signature` da URL pré-assinada).
pub fn canonical_query(q: &str, skip: Option<&str>) -> String {
    let mut pairs: Vec<(String, String)> = query_pairs(q)
        .into_iter()
        .map(|(k, v)| {
            (
                uri_encode(&percent_decode(&k), true),
                uri_encode(&percent_decode(&v), true),
            )
        })
        .filter(|(k, _)| skip.is_none_or(|s| k != s))
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// O valor canônico de um cabeçalho: os valores juntados com `,`, sem os
/// espaços das pontas e com os seguidos reduzidos a um.
pub fn header_value<'a>(values: impl Iterator<Item = &'a str>) -> String {
    values
        .map(|v| v.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join(",")
}

/// Um pedido como o assinador o vê.
pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    /// Cabeçalhos com o nome em minúsculas.
    pub headers: &'a [(String, String)],
    pub payload_hash: &'a str,
}

/// O pedido canônico com os cabeçalhos de `signed`.
pub fn canonical_request(r: &Request, signed: &[String], s3: bool, skip: Option<&str>) -> String {
    let mut ch = String::new();
    for name in signed {
        let v = header_value(
            r.headers
                .iter()
                .filter(|(k, _)| k == name)
                .map(|(_, v)| v.as_str()),
        );
        ch.push_str(&format!("{name}:{v}\n"));
    }
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        r.method,
        canonical_uri(r.path, s3),
        canonical_query(r.query, skip),
        ch,
        signed.join(";"),
        r.payload_hash
    )
}

/// A assinatura de um pedido canônico com a chave derivada.
pub fn sign(key: &[u8], amz_date: &str, scope: &Scope, canonical: &str) -> String {
    let sts = format!(
        "{ALGORITHM}\n{amz_date}\n{}\n{}",
        scope.text(),
        sha256_hex(canonical.as_bytes())
    );
    hex(&hmac(key, sts.as_bytes()))
}

/// As assinaturas dos pedaços de um `aws-chunked` assinado, encadeadas a
/// partir da assinatura semente (a do pedido).
pub struct ChunkSigner {
    key: Vec<u8>,
    amz_date: String,
    scope: Scope,
    prev: String,
}

impl ChunkSigner {
    pub fn new(key: Vec<u8>, amz_date: &str, scope: Scope, seed: &str) -> ChunkSigner {
        ChunkSigner {
            key,
            amz_date: amz_date.into(),
            scope,
            prev: seed.into(),
        }
    }

    /// A assinatura do próximo pedaço.
    pub fn chunk(&mut self, data: &[u8]) -> String {
        let sts = format!(
            "AWS4-HMAC-SHA256-PAYLOAD\n{}\n{}\n{}\n{EMPTY_HASH}\n{}",
            self.amz_date,
            self.scope.text(),
            self.prev,
            sha256_hex(data)
        );
        self.prev = hex(&hmac(&self.key, sts.as_bytes()));
        self.prev.clone()
    }

    /// A assinatura do trailer (`nome:valor\n` de cada cabeçalho).
    pub fn trailer(&mut self, canonical: &str) -> String {
        let sts = format!(
            "AWS4-HMAC-SHA256-TRAILER\n{}\n{}\n{}\n{}",
            self.amz_date,
            self.scope.text(),
            self.prev,
            sha256_hex(canonical.as_bytes())
        );
        self.prev = hex(&hmac(&self.key, sts.as_bytes()));
        self.prev.clone()
    }
}

/// Reescreve um corpo `aws-chunked` assinado em fluxo: cada pedaço é
/// guardado até o fim (a assinatura vem antes dos dados) e sai com a
/// assinatura refeita; o tamanho não muda. Um corpo fora do formato passa
/// sem mudar daí em diante.
pub struct ChunkRewriter {
    signer: ChunkSigner,
    buf: Vec<u8>,
    state: State,
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    /// Esperando a linha `tamanho;chunk-signature=…`.
    Header,
    /// Os dados (e o `\r\n`) de um pedaço de `n` bytes.
    Data(usize),
    /// Depois do pedaço final: o trailer até a linha vazia.
    Trailer,
    /// Fim, ou fora do formato: passa o resto sem mudar.
    Pass,
}

/// O maior pedaço guardado.
const MAX_CHUNK: usize = 16 << 20;

impl ChunkRewriter {
    pub fn new(signer: ChunkSigner) -> ChunkRewriter {
        ChunkRewriter {
            signer,
            buf: Vec::new(),
            state: State::Header,
        }
    }

    /// Recebe bytes do corpo e devolve os que já podem seguir.
    pub fn push(&mut self, input: &[u8]) -> Vec<u8> {
        if self.state == State::Pass {
            return input.to_vec();
        }
        self.buf.extend_from_slice(input);
        let mut out = Vec::new();
        loop {
            match self.state {
                State::Pass => {
                    out.append(&mut self.buf);
                    return out;
                }
                State::Header => {
                    let Some(end) = find(&self.buf, b"\r\n") else {
                        if self.buf.len() > 1024 {
                            self.state = State::Pass;
                            continue;
                        }
                        return out;
                    };
                    let line = String::from_utf8_lossy(&self.buf[..end]).into_owned();
                    let size = line
                        .split_once(";chunk-signature=")
                        .and_then(|(h, _)| usize::from_str_radix(h.trim(), 16).ok());
                    match size {
                        Some(n) if n <= MAX_CHUNK => self.state = State::Data(n),
                        _ => self.state = State::Pass,
                    }
                }
                State::Data(0) => {
                    // O pedaço final não tem dados nem o `\r\n` deles: vem
                    // o trailer, ou só a linha vazia.
                    let end = find(&self.buf, b"\r\n").unwrap_or(0) + 2;
                    let sig = self.signer.chunk(b"");
                    out.extend_from_slice(format!("0;chunk-signature={sig}\r\n").as_bytes());
                    self.buf.drain(..end);
                    self.state = State::Trailer;
                }
                State::Data(n) => {
                    let end = find(&self.buf, b"\r\n").unwrap_or(0) + 2;
                    if self.buf.len() < end + n + 2 {
                        return out;
                    }
                    let data = &self.buf[end..end + n];
                    let sig = self.signer.chunk(data);
                    out.extend_from_slice(format!("{n:x};chunk-signature={sig}\r\n").as_bytes());
                    out.extend_from_slice(&self.buf[end..end + n + 2]);
                    self.buf.drain(..end + n + 2);
                    self.state = State::Header;
                }
                State::Trailer => {
                    // Sem trailer, o corpo acaba no `\r\n` logo depois do
                    // pedaço final.
                    if self.buf.starts_with(b"\r\n") {
                        self.state = State::Pass;
                        continue;
                    }
                    let Some(end) = find(&self.buf, b"\r\n\r\n") else {
                        if self.buf.len() > 64 * 1024 {
                            self.state = State::Pass;
                        }
                        if self.state != State::Pass {
                            return out;
                        }
                        continue;
                    };
                    let text = String::from_utf8_lossy(&self.buf[..end]).into_owned();
                    let mut canonical = String::new();
                    let mut lines = Vec::new();
                    for l in text.split("\r\n") {
                        let Some((k, v)) = l.split_once(':') else {
                            continue;
                        };
                        let k = k.trim().to_ascii_lowercase();
                        if k == "x-amz-trailer-signature" {
                            continue;
                        }
                        canonical.push_str(&format!("{k}:{}\n", v.trim()));
                        lines.push(l.to_string());
                    }
                    let sig = self.signer.trailer(&canonical);
                    for l in lines {
                        out.extend_from_slice(l.as_bytes());
                        out.extend_from_slice(b"\r\n");
                    }
                    out.extend_from_slice(
                        format!("x-amz-trailer-signature:{sig}\r\n\r\n").as_bytes(),
                    );
                    self.buf.drain(..end + 4);
                    self.state = State::Pass;
                }
            }
        }
    }

    /// O que sobrou no fim do corpo.
    pub fn finish(&mut self) -> Vec<u8> {
        self.state = State::Pass;
        std::mem::take(&mut self.buf)
    }
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn exemplo_get_vanilla() {
        let headers = h(&[
            ("host", "example.amazonaws.com"),
            ("x-amz-date", "20150830T123600Z"),
        ]);
        let r = Request {
            method: "GET",
            path: "/",
            query: "",
            headers: &headers,
            payload_hash: EMPTY_HASH,
        };
        let signed = vec!["host".to_string(), "x-amz-date".to_string()];
        let scope = Scope {
            date: "20150830".into(),
            region: "us-east-1".into(),
            service: "service".into(),
        };
        let c = canonical_request(&r, &signed, false, None);
        let key = scope.key("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY");
        assert_eq!(
            sign(&key, "20150830T123600Z", &scope, &c),
            "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    fn s3_scope() -> Scope {
        Scope {
            date: "20130524".into(),
            region: "us-east-1".into(),
            service: "s3".into(),
        }
    }

    const S3_SECRET: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";

    #[test]
    fn exemplo_get_object_do_s3() {
        let headers = h(&[
            ("host", "examplebucket.s3.amazonaws.com"),
            ("range", "bytes=0-9"),
            ("x-amz-content-sha256", EMPTY_HASH),
            ("x-amz-date", "20130524T000000Z"),
        ]);
        let r = Request {
            method: "GET",
            path: "/test.txt",
            query: "",
            headers: &headers,
            payload_hash: EMPTY_HASH,
        };
        let signed: Vec<String> = ["host", "range", "x-amz-content-sha256", "x-amz-date"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let c = canonical_request(&r, &signed, true, None);
        assert_eq!(
            sign(
                &s3_scope().key(S3_SECRET),
                "20130524T000000Z",
                &s3_scope(),
                &c
            ),
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    fn chunked_seed() -> (String, Vec<u8>) {
        let headers = h(&[
            ("content-encoding", "aws-chunked"),
            ("content-length", "66824"),
            ("host", "s3.amazonaws.com"),
            ("x-amz-content-sha256", STREAMING),
            ("x-amz-date", "20130524T000000Z"),
            ("x-amz-decoded-content-length", "66560"),
            ("x-amz-storage-class", "REDUCED_REDUNDANCY"),
        ]);
        let signed: Vec<String> = headers.iter().map(|(k, _)| k.clone()).collect();
        let r = Request {
            method: "PUT",
            path: "/examplebucket/chunkObject.txt",
            query: "",
            headers: &headers,
            payload_hash: STREAMING,
        };
        let key = s3_scope().key(S3_SECRET);
        let c = canonical_request(&r, &signed, true, None);
        (sign(&key, "20130524T000000Z", &s3_scope(), &c), key)
    }

    #[test]
    fn exemplo_dos_pedacos_assinados() {
        let (seed, key) = chunked_seed();
        assert_eq!(
            seed,
            "4f232c4386841ef735655705268965c44a0e4690baa4adea153f7db9fa80a0a9"
        );
        let mut s = ChunkSigner::new(key, "20130524T000000Z", s3_scope(), &seed);
        assert_eq!(
            s.chunk(&[b'a'; 65536]),
            "ad80c730a21e5b8d04586a2213dd63b9a0e99e0e2307b0ade35a65485a288648"
        );
        assert_eq!(
            s.chunk(&[b'a'; 1024]),
            "0055627c9e194cb4542bae2aa5492e3c1575bbb81b612b7d234b86a503ef5497"
        );
        assert_eq!(
            s.chunk(b""),
            "b6c6ea8a5354eaf15b3cb7646744f4275b71ea724fed81ceb9323e279d449df9"
        );
    }

    #[test]
    fn corpo_reescrito_em_leituras_de_tamanhos_variados() {
        let (seed, key) = chunked_seed();
        // O corpo como o SDK mandaria com uma chave falsa (assinaturas erradas).
        let bad = "0".repeat(64);
        let mut body = Vec::new();
        for n in [65536usize, 1024, 0] {
            body.extend_from_slice(format!("{n:x};chunk-signature={bad}\r\n").as_bytes());
            body.extend(std::iter::repeat_n(b'a', n));
            body.extend_from_slice(b"\r\n");
        }
        for step in [1usize, 7, 1000, 70000] {
            let mut rw = ChunkRewriter::new(ChunkSigner::new(
                key.clone(),
                "20130524T000000Z",
                s3_scope(),
                &seed,
            ));
            let mut out = Vec::new();
            for piece in body.chunks(step) {
                out.extend(rw.push(piece));
            }
            out.extend(rw.finish());
            assert_eq!(out.len(), body.len(), "passo {step}");
            let text = String::from_utf8_lossy(&out);
            assert!(text.starts_with(
                "10000;chunk-signature=ad80c730a21e5b8d04586a2213dd63b9a0e99e0e2307b0ade35a65485a288648\r\n"
            ));
            assert!(text.contains(
                "400;chunk-signature=0055627c9e194cb4542bae2aa5492e3c1575bbb81b612b7d234b86a503ef5497\r\n"
            ));
            assert!(text.ends_with(
                "0;chunk-signature=b6c6ea8a5354eaf15b3cb7646744f4275b71ea724fed81ceb9323e279d449df9\r\n\r\n"
            ));
        }
    }

    #[test]
    fn trailer_reassinado() {
        let (seed, key) = chunked_seed();
        let bad = "0".repeat(64);
        let body = format!(
            "3;chunk-signature={bad}\r\nabc\r\n0;chunk-signature={bad}\r\nx-amz-checksum-crc32:NSRBwg==\r\nx-amz-trailer-signature:{bad}\r\n\r\n"
        );
        let mut rw = ChunkRewriter::new(ChunkSigner::new(
            key.clone(),
            "20130524T000000Z",
            s3_scope(),
            &seed,
        ));
        let mut out = rw.push(body.as_bytes());
        out.extend(rw.finish());
        assert_eq!(out.len(), body.len());
        let mut s = ChunkSigner::new(key, "20130524T000000Z", s3_scope(), &seed);
        let (c1, c0) = (s.chunk(b"abc"), s.chunk(b""));
        let t = s.trailer("x-amz-checksum-crc32:NSRBwg==\n");
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(&c1) && text.contains(&c0), "{text}");
        assert!(
            text.ends_with(&format!("x-amz-trailer-signature:{t}\r\n\r\n")),
            "{text}"
        );
    }

    #[test]
    fn caminho_e_query_canonicos() {
        // Nos serviços fora do S3, o caminho já codificado é codificado de novo.
        assert_eq!(canonical_uri("/a%20b/c", false), "/a%2520b/c");
        assert_eq!(canonical_uri("/a%20b/c", true), "/a%20b/c");
        assert_eq!(canonical_uri("", false), "/");
        assert_eq!(
            canonical_query("b=2&a=x+y&a=1&c&d=%7e%2a", None),
            "a=1&a=x%2By&b=2&c=&d=~%2A"
        );
        assert_eq!(
            canonical_query("X-Amz-Signature=abc&X-Amz-Date=1", Some("X-Amz-Signature")),
            "X-Amz-Date=1"
        );
        assert_eq!(header_value(["  a   b ", "c"].into_iter()), "a b,c");
        assert_eq!(percent_decode("a%2Fb%"), b"a/b%");
    }

    #[test]
    fn authorization_lido_e_refeito() {
        let a = Auth::parse(
            "AWS4-HMAC-SHA256 Credential=AKIASANDMUXABCDEFGHI/20260101/sa-east-1/s3/aws4_request, SignedHeaders=host;x-amz-date, Signature=abc",
        )
        .unwrap();
        assert_eq!(a.access_key, "AKIASANDMUXABCDEFGHI");
        assert_eq!(a.scope.service, "s3");
        assert_eq!(a.signed, vec!["host", "x-amz-date"]);
        assert_eq!(Auth::parse(&a.header()), Some(a));
        assert!(Auth::parse("Bearer x").is_none());
        assert!(parse_credential("A/1/r/s/aws4_requestX").is_none());
    }
}
