//! Enquadramento: 4 bytes de tamanho, 1 de tipo, conteúdo.

use std::io::{Read, Write};

use crate::{Msg, ProtoError};

/// Maior conteúdo aceito num quadro.
pub const MAX_FRAME: usize = 16 << 20;

const KIND_CONTROL: u8 = 0;
const KIND_DATA: u8 = 1;
const HEADER: usize = 5;

/// Um quadro decodificado.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Control(Msg),
    Data(Vec<u8>),
}

fn encode(kind: u8, payload: &[u8]) -> Result<Vec<u8>, ProtoError> {
    if payload.len() > MAX_FRAME {
        return Err(ProtoError::TooLarge(payload.len()));
    }
    let mut out = Vec::with_capacity(HEADER + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.push(kind);
    out.extend_from_slice(payload);
    Ok(out)
}

impl Frame {
    /// Bytes do quadro pronto para escrever.
    pub fn encode(&self) -> Result<Vec<u8>, ProtoError> {
        match self {
            Frame::Control(m) => {
                let json =
                    serde_json::to_vec(m).map_err(|e| ProtoError::BadMessage(e.to_string()))?;
                encode(KIND_CONTROL, &json)
            }
            Frame::Data(d) => encode(KIND_DATA, d),
        }
    }

    fn decode(kind: u8, payload: Vec<u8>) -> Result<Frame, ProtoError> {
        match kind {
            KIND_CONTROL => serde_json::from_slice(&payload)
                .map(Frame::Control)
                .map_err(|e| ProtoError::BadMessage(e.to_string())),
            KIND_DATA => Ok(Frame::Data(payload)),
            k => Err(ProtoError::UnknownKind(k)),
        }
    }
}

/// Escreve uma mensagem de controle.
pub fn write_control(w: &mut impl Write, msg: &Msg) -> Result<(), ProtoError> {
    w.write_all(&Frame::Control(msg.clone()).encode()?)?;
    Ok(())
}

/// Escreve bytes de terminal.
pub fn write_data(w: &mut impl Write, data: &[u8]) -> Result<(), ProtoError> {
    w.write_all(&encode(KIND_DATA, data)?)?;
    Ok(())
}

/// Lê um quadro de uma fonte bloqueante. `None` quando a conexão termina
/// entre quadros.
pub fn read_frame(r: &mut impl Read) -> Result<Option<Frame>, ProtoError> {
    let mut header = [0u8; HEADER];
    let mut got = 0;
    while got < HEADER {
        match r.read(&mut header[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(ProtoError::Truncated),
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
    if len > MAX_FRAME {
        return Err(ProtoError::TooLarge(len));
    }
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            ProtoError::Truncated
        } else {
            e.into()
        }
    })?;
    Frame::decode(header[4], payload).map(Some)
}

/// Decodificador incremental, para bytes que chegam em pedaços.
#[derive(Debug, Default)]
pub struct Decoder {
    buf: Vec<u8>,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// O próximo quadro completo, se já chegou inteiro.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, ProtoError> {
        if self.buf.len() < HEADER {
            return Ok(None);
        }
        let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        if len > MAX_FRAME {
            return Err(ProtoError::TooLarge(len));
        }
        if self.buf.len() < HEADER + len {
            return Ok(None);
        }
        let kind = self.buf[4];
        let payload = self.buf[HEADER..HEADER + len].to_vec();
        self.buf.drain(..HEADER + len);
        Frame::decode(kind, payload).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LaunchOptions, Size};

    fn sample() -> Vec<Frame> {
        vec![
            Frame::Control(Msg::List),
            Frame::Data(b"\x1b[1mol\xc3\xa1\x00".to_vec()),
            Frame::Control(Msg::Create {
                opts: LaunchOptions::new("shell"),
                vars: [("TERM".to_string(), "xterm".to_string())].into(),
                size: Size { cols: 80, rows: 24 },
                attach: true,
            }),
            Frame::Data(vec![]),
        ]
    }

    #[test]
    fn ida_e_volta_bloqueante() {
        let mut bytes = Vec::new();
        for f in sample() {
            bytes.extend(f.encode().unwrap());
        }
        let mut r = &bytes[..];
        let mut out = Vec::new();
        while let Some(f) = read_frame(&mut r).unwrap() {
            out.push(f);
        }
        assert_eq!(out, sample());
    }

    #[test]
    fn quadro_dividido_em_qualquer_ponto() {
        let mut bytes = Vec::new();
        for f in sample() {
            bytes.extend(f.encode().unwrap());
        }
        for cut in 0..bytes.len() {
            let mut d = Decoder::new();
            let mut out = Vec::new();
            d.push(&bytes[..cut]);
            while let Some(f) = d.next_frame().unwrap() {
                out.push(f);
            }
            d.push(&bytes[cut..]);
            while let Some(f) = d.next_frame().unwrap() {
                out.push(f);
            }
            assert_eq!(out, sample(), "corte em {cut}");
        }
    }

    #[test]
    fn quadro_acima_do_limite_recusado() {
        let mut header = ((MAX_FRAME + 1) as u32).to_be_bytes().to_vec();
        header.push(1);
        let mut d = Decoder::new();
        d.push(&header);
        assert!(matches!(d.next_frame(), Err(ProtoError::TooLarge(_))));
        assert!(matches!(
            read_frame(&mut &header[..]),
            Err(ProtoError::TooLarge(_))
        ));
        assert!(matches!(
            encode(KIND_DATA, &vec![0u8; MAX_FRAME + 1]),
            Err(ProtoError::TooLarge(_))
        ));
    }

    #[test]
    fn quadro_truncado_e_tipo_desconhecido() {
        let bytes = Frame::Data(b"abc".to_vec()).encode().unwrap();
        assert!(matches!(
            read_frame(&mut &bytes[..6]),
            Err(ProtoError::Truncated)
        ));
        let mut bad = bytes.clone();
        bad[4] = 9;
        assert!(matches!(
            read_frame(&mut &bad[..]),
            Err(ProtoError::UnknownKind(9))
        ));
    }
}
