//! WebSocket numa conexão decifrada (RFC 6455): as mensagens de texto do
//! sandbox ao destino passam por um leitor de quadros que troca o valor falso
//! de um segredo pelo real (o Discord manda o token no IDENTIFY). O sentido do
//! destino ao sandbox é uma cópia direta.

use std::io;

use sandmux_proto::msg::Injection;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Maior quadro aceito; um maior fecha a conexão.
pub const MAX_FRAME: u64 = 16 << 20;

/// Um quadro lido do sandbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsFrame {
    /// Primeiro byte: FIN, RSV1-3 e opcode.
    pub head: u8,
    pub mask: Option<[u8; 4]>,
    /// Conteúdo já desmascarado.
    pub payload: Vec<u8>,
}

impl WsFrame {
    pub fn opcode(&self) -> u8 {
        self.head & 0x0f
    }

    /// RSV1: conteúdo comprimido (permessage-deflate).
    pub fn compressed(&self) -> bool {
        self.head & 0x40 != 0
    }

    /// Codifica o quadro, mascarando com a mesma chave.
    pub fn encode(&self) -> Vec<u8> {
        let n = self.payload.len();
        let mut out = Vec::with_capacity(n + 14);
        out.push(self.head);
        let m = if self.mask.is_some() { 0x80 } else { 0 };
        if n < 126 {
            out.push(m | n as u8);
        } else if n <= u16::MAX as usize {
            out.push(m | 126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        } else {
            out.push(m | 127);
            out.extend_from_slice(&(n as u64).to_be_bytes());
        }
        match self.mask {
            Some(k) => {
                out.extend_from_slice(&k);
                out.extend(self.payload.iter().enumerate().map(|(i, b)| b ^ k[i % 4]));
            }
            None => out.extend_from_slice(&self.payload),
        }
        out
    }
}

/// Lê um quadro; `None` no fim limpo do fluxo.
pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> io::Result<Option<WsFrame>> {
    let mut h = [0u8; 2];
    match r.read_exact(&mut h[..1]).await {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    r.read_exact(&mut h[1..]).await?;
    let masked = h[1] & 0x80 != 0;
    let len = match h[1] & 0x7f {
        126 => {
            let mut b = [0u8; 2];
            r.read_exact(&mut b).await?;
            u16::from_be_bytes(b) as u64
        }
        127 => {
            let mut b = [0u8; 8];
            r.read_exact(&mut b).await?;
            u64::from_be_bytes(b)
        }
        n => n as u64,
    };
    if len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            sandmux_spec::tr!(
                "quadro WebSocket grande demais",
                "WebSocket frame too large"
            ),
        ));
    }
    let mask = if masked {
        let mut k = [0u8; 4];
        r.read_exact(&mut k).await?;
        Some(k)
    } else {
        None
    };
    let mut payload = vec![0u8; len as usize];
    r.read_exact(&mut payload).await?;
    if let Some(k) = mask {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= k[i % 4];
        }
    }
    Ok(Some(WsFrame {
        head: h[0],
        mask,
        payload,
    }))
}

fn replace_bytes(hay: &[u8], needle: &[u8], with: &[u8]) -> Option<Vec<u8>> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let mut out = Vec::with_capacity(hay.len());
    let mut i = 0;
    let mut hit = false;
    while i < hay.len() {
        if hay[i..].starts_with(needle) {
            out.extend_from_slice(with);
            i += needle.len();
            hit = true;
        } else {
            out.push(hay[i]);
            i += 1;
        }
    }
    hit.then_some(out)
}

/// Troca os falsos pelos reais no conteúdo. Devolve o nome do primeiro
/// segredo usado.
pub fn swap(payload: &mut Vec<u8>, list: &[Injection]) -> Option<String> {
    let mut used = None;
    for i in list {
        if let Some(out) = replace_bytes(payload, i.fake.as_bytes(), i.value.as_bytes()) {
            *payload = out;
            used.get_or_insert_with(|| i.name.clone());
        }
    }
    used
}

/// Repassa os quadros do sandbox ao destino, trocando os falsos nas mensagens
/// de texto. Devolve os bytes lidos do sandbox e o segredo usado.
pub async fn pump_client<R, W>(mut from: R, mut to: W, list: &[Injection]) -> (u64, Option<String>)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut n = 0u64;
    let mut used = None;
    // A mensagem em curso é de texto (as continuações herdam o tipo).
    let mut text = false;
    while let Ok(Some(mut f)) = read_frame(&mut from).await {
        n += f.payload.len() as u64;
        match f.opcode() {
            1 => text = true,
            2 => text = false,
            _ => {}
        }
        let data = matches!(f.opcode(), 0..=2);
        if data
            && text
            && !f.compressed()
            && let Some(s) = swap(&mut f.payload, list)
        {
            used.get_or_insert(s);
        }
        if to.write_all(&f.encode()).await.is_err() {
            break;
        }
        if f.opcode() == 8 {
            break;
        }
    }
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), to.shutdown()).await;
    (n, used)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inj(fake: &str, value: &str) -> Injection {
        Injection {
            name: "discord".into(),
            domains: vec!["gateway.discord.gg".into()],
            fake: fake.into(),
            value: value.into(),
        }
    }

    #[tokio::test]
    async fn quadro_ida_e_volta() {
        for n in [0usize, 5, 125, 126, 300, 70_000] {
            let f = WsFrame {
                head: 0x81,
                mask: Some([1, 2, 3, 4]),
                payload: vec![b'a'; n],
            };
            let bytes = f.encode();
            let mut r = &bytes[..];
            assert_eq!(read_frame(&mut r).await.unwrap(), Some(f));
        }
        let mut empty: &[u8] = &[];
        assert_eq!(read_frame(&mut empty).await.unwrap(), None);
    }

    #[tokio::test]
    async fn troca_no_texto_e_reemite_com_o_tamanho_novo() {
        let fake = "sandmux-fake-0123";
        // O real é maior e cruza a fronteira dos 126 bytes.
        let real = "R".repeat(200);
        let msg = format!(r#"{{"op":2,"d":{{"token":"{fake}"}}}}"#);
        let mut input = Vec::new();
        input.extend(
            WsFrame {
                head: 0x81,
                mask: Some([9, 8, 7, 6]),
                payload: msg.clone().into_bytes(),
            }
            .encode(),
        );
        // Binário com o falso: passa intacto.
        input.extend(
            WsFrame {
                head: 0x82,
                mask: Some([1, 1, 1, 1]),
                payload: fake.as_bytes().to_vec(),
            }
            .encode(),
        );
        // Comprimido: passa intacto.
        input.extend(
            WsFrame {
                head: 0xc1,
                mask: Some([1, 1, 1, 1]),
                payload: fake.as_bytes().to_vec(),
            }
            .encode(),
        );
        let mut out = Vec::new();
        let (n, used) = pump_client(&input[..], &mut out, &[inj(fake, &real)]).await;
        assert_eq!(used.as_deref(), Some("discord"));
        assert!(n > 0);
        let mut r = &out[..];
        let a = read_frame(&mut r).await.unwrap().unwrap();
        assert_eq!(
            String::from_utf8(a.payload).unwrap(),
            msg.replace(fake, &real)
        );
        assert_eq!(a.mask, Some([9, 8, 7, 6]));
        let b = read_frame(&mut r).await.unwrap().unwrap();
        assert_eq!(b.payload, fake.as_bytes());
        let c = read_frame(&mut r).await.unwrap().unwrap();
        assert_eq!(c.payload, fake.as_bytes());
    }

    #[tokio::test]
    async fn quadro_grande_demais_fecha() {
        let mut h = vec![0x81u8, 0x80 | 127];
        h.extend_from_slice(&(MAX_FRAME + 1).to_be_bytes());
        let mut r = &h[..];
        assert!(read_frame(&mut r).await.is_err());
    }
}
