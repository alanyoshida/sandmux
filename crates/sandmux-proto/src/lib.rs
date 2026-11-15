//! Protocolo entre o cliente `sandmux` e o daemon `sandmuxd`, e utilitários
//! de fluxo de terminal usados pelos dois lados.
//!
//! O transporte é um socket Unix de fluxo. Cada quadro tem 4 bytes de tamanho
//! (big-endian, só o conteúdo), 1 byte de tipo e o conteúdo: tipo 0 é uma
//! mensagem de controle em JSON ([`Msg`]); tipo 1 são bytes de terminal.

pub mod frame;
pub mod msg;
pub mod pane;
pub mod term;

pub use frame::{Decoder, Frame, MAX_FRAME, read_frame, write_control, write_data};
pub use msg::{LaunchOptions, Msg, SessionInfo, SessionState, Size};

/// Versão do protocolo. Muda a cada alteração incompatível; cliente e daemon
/// exigem igualdade exata.
pub const PROTOCOL: u32 = 33;

/// Erros de leitura e escrita do protocolo.
#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    TooLarge(usize),
    UnknownKind(u8),
    BadMessage(String),
    Truncated,
    Io(#[from] std::io::Error),
}

impl std::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use sandmux_spec::{tr, trf};
        match self {
            ProtoError::TooLarge(n) => f.write_str(&trf!(
                "quadro de {n} bytes passa do limite de {MAX_FRAME}",
                "frame of {n} bytes exceeds the limit of {MAX_FRAME}"
            )),
            ProtoError::UnknownKind(k) => f.write_str(&trf!(
                "tipo de quadro desconhecido: {k}",
                "unknown frame type: {k}"
            )),
            ProtoError::BadMessage(m) => f.write_str(&trf!(
                "mensagem de controle inválida: {m}",
                "invalid control message: {m}"
            )),
            ProtoError::Truncated => f.write_str(tr!(
                "conexão encerrada no meio de um quadro",
                "connection closed in the middle of a frame"
            )),
            ProtoError::Io(e) => write!(f, "{e}"),
        }
    }
}
