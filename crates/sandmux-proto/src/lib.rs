//! Protocolo entre o cliente `sandmux` e o daemon `sandmuxd`, e utilitários
//! de fluxo de terminal usados pelos dois lados.
//!
//! O transporte é um socket Unix de fluxo. Cada quadro tem 4 bytes de tamanho
//! (big-endian, só o conteúdo), 1 byte de tipo e o conteúdo: tipo 0 é uma
//! mensagem de controle em JSON ([`Msg`]); tipo 1 são bytes de terminal.

pub mod frame;
pub mod msg;
pub mod term;

pub use frame::{Decoder, Frame, MAX_FRAME, read_frame, write_control, write_data};
pub use msg::{LaunchOptions, Msg, SessionInfo, SessionState, Size};

/// Versão do protocolo. Muda a cada alteração incompatível; cliente e daemon
/// exigem igualdade exata.
pub const PROTOCOL: u32 = 2;

/// Erros de leitura e escrita do protocolo.
#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("quadro de {0} bytes passa do limite de {MAX_FRAME}")]
    TooLarge(usize),
    #[error("tipo de quadro desconhecido: {0}")]
    UnknownKind(u8),
    #[error("mensagem de controle inválida: {0}")]
    BadMessage(String),
    #[error("conexão encerrada no meio de um quadro")]
    Truncated,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}
