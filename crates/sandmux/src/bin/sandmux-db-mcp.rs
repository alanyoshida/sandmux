//! Repassador do MCP de banco dentro do sandbox: copia o stdin para o socket
//! do daemon (`SANDMUX_DB_SOCKET`) e o socket para o stdout. A conexão e a
//! senha ficam no daemon, fora do sandbox.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::var_os("SANDMUX_DB_SOCKET") else {
        eprintln!("sandmux-db-mcp: falta SANDMUX_DB_SOCKET");
        return ExitCode::from(2);
    };
    let sock = match UnixStream::connect(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("sandmux-db-mcp: {}: {e}", path.to_string_lossy());
            return ExitCode::from(1);
        }
    };
    let Ok(mut to_daemon) = sock.try_clone() else {
        return ExitCode::from(1);
    };
    // stdin → socket; no fim do stdin, fecha a escrita para o daemon.
    std::thread::spawn(move || {
        let mut buf = [0u8; 64 * 1024];
        let mut stdin = std::io::stdin().lock();
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if to_daemon.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            }
        }
        let _ = to_daemon.shutdown(std::net::Shutdown::Write);
    });
    // socket → stdout, até o daemon fechar.
    let mut from_daemon = sock;
    let mut out = std::io::stdout().lock();
    let mut buf = [0u8; 64 * 1024];
    loop {
        match from_daemon.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if out.write_all(&buf[..n]).and_then(|_| out.flush()).is_err() {
                    break;
                }
            }
        }
    }
    ExitCode::SUCCESS
}
