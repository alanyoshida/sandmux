//! Um terminal de teste: roda um comando num PTY, escreve teclas e lê a tela
//! por um emulador em memória.
#![allow(dead_code)]

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct Term {
    /// O único descritor do mestre; `None` depois de fechado.
    master: Arc<Mutex<Option<File>>>,
    parser: Arc<Mutex<vt100::Parser>>,
    raw: Arc<Mutex<Vec<u8>>>,
    pub child: Child,
}

fn winsize(cols: u16, rows: u16) -> libc::winsize {
    libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

impl Term {
    /// Roda `cmd` num terminal de `cols` x `rows`.
    pub fn spawn(cmd: Command, cols: u16, rows: u16) -> Term {
        Self::spawn_with(cmd, cols, rows, false)
    }

    /// Como [`Term::spawn`], respondendo às consultas mais comuns ao terminal
    /// (atributos, posição do cursor, teclado do kitty, cores), como um
    /// terminal real.
    pub fn spawn_answering(cmd: Command, cols: u16, rows: u16) -> Term {
        Self::spawn_with(cmd, cols, rows, true)
    }

    fn spawn_with(mut cmd: Command, cols: u16, rows: u16, answer: bool) -> Term {
        let ws = nix::pty::Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let pty = nix::pty::openpty(Some(&ws), None).unwrap();
        // Testes rodam em paralelo: o PTY de um teste não pode vazar para os
        // processos de outro.
        for fd in [pty.master.as_raw_fd(), pty.slave.as_raw_fd()] {
            // SAFETY: fcntl sobre descritores válidos.
            unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
        }
        let slave: OwnedFd = pty.slave;
        cmd.stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave));
        // SAFETY: setsid e ioctl depois de fork.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                libc::ioctl(0, libc::TIOCSCTTY, 0);
                Ok(())
            });
        }
        let child = cmd.spawn().unwrap();
        drop(cmd);
        let master = Arc::new(Mutex::new(Some(File::from(pty.master))));
        let fd = master.lock().unwrap().as_ref().unwrap().as_raw_fd();
        let parser = Arc::new(Mutex::new(vt100::Parser::new(
            std::num::NonZeroU16::new(rows).unwrap(),
            std::num::NonZeroU16::new(cols).unwrap(),
            20_000,
        )));
        let raw = Arc::new(Mutex::new(Vec::new()));
        {
            let master = master.clone();
            let parser = parser.clone();
            let raw = raw.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 65536];
                loop {
                    let mut pfd = libc::pollfd {
                        fd,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    // SAFETY: poll sobre uma estrutura válida.
                    unsafe { libc::poll(&mut pfd, 1, 50) };
                    let mut guard = master.lock().unwrap();
                    let Some(m) = guard.as_mut() else { break };
                    if pfd.revents == 0 {
                        continue;
                    }
                    match m.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let mut p = parser.lock().unwrap();
                            p.process(&buf[..n]);
                            let mut r = raw.lock().unwrap();
                            let from = r.len().saturating_sub(32);
                            r.extend_from_slice(&buf[..n]);
                            if answer {
                                let reply = answers(&r[from..], r.len() - from - n, p.screen());
                                if !reply.is_empty() {
                                    let _ = m.write_all(&reply);
                                }
                            }
                        }
                    }
                }
            });
        }
        Term {
            master,
            parser,
            raw,
            child,
        }
    }

    pub fn write(&mut self, bytes: &[u8]) {
        let mut g = self.master.lock().unwrap();
        g.as_mut().unwrap().write_all(bytes).unwrap();
    }

    /// Texto visível da tela.
    pub fn screen(&self) -> String {
        self.parser.lock().unwrap().screen().contents()
    }

    /// Tudo o que o terminal recebeu, cru.
    pub fn raw(&self) -> Vec<u8> {
        self.raw.lock().unwrap().clone()
    }

    pub fn raw_text(&self) -> String {
        String::from_utf8_lossy(&self.raw()).into_owned()
    }

    /// Linhas da rolagem do terminal mais as da tela.
    pub fn scrollback_text(&self) -> String {
        let mut p = self.parser.lock().unwrap();
        let screen = p.screen_mut();
        screen.set_scrollback(usize::MAX);
        let total = screen.scrollback();
        let (rows, cols) = screen.size();
        let mut lines = Vec::new();
        let mut done = 0;
        while done < total {
            screen.set_scrollback(total - done);
            let take = usize::from(rows.get()).min(total - done);
            lines.extend(screen.rows(0, cols.get()).take(take));
            done += take;
        }
        screen.set_scrollback(0);
        lines.extend(screen.rows(0, cols.get()));
        lines.join("\n")
    }

    pub fn with_screen<T>(&self, f: impl FnOnce(&vt100::Screen) -> T) -> T {
        f(self.parser.lock().unwrap().screen())
    }

    /// Espera a tela satisfazer `pred`.
    pub fn wait_for(&self, what: &str, pred: impl Fn(&str) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let s = self.screen();
            if pred(&s) {
                return;
            }
            if Instant::now() > deadline {
                panic!(
                    "esperando {what}; tela:\n{s}\n--- recebido:\n{:?}",
                    self.raw_text()
                );
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    pub fn wait_text(&self, text: &str) {
        self.wait_for(text, |s| s.contains(text));
    }

    /// Espera os bytes crus conterem `text`.
    pub fn wait_raw(&self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !self.raw_text().contains(text) {
            if Instant::now() > deadline {
                panic!("esperando {text:?} na saída crua: {:?}", self.raw_text());
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let ws = winsize(cols, rows);
        let g = self.master.lock().unwrap();
        // SAFETY: ioctl com estrutura winsize válida no mestre do PTY.
        unsafe { libc::ioctl(g.as_ref().unwrap().as_raw_fd(), libc::TIOCSWINSZ, &ws) };
        drop(g);
        self.parser.lock().unwrap().screen_mut().set_size(
            std::num::NonZeroU16::new(rows).unwrap(),
            std::num::NonZeroU16::new(cols).unwrap(),
        );
    }

    /// Espera o comando terminar e devolve o código.
    pub fn wait_exit(&mut self) -> i32 {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(st) = self.child.try_wait().unwrap() {
                // Deixa a leitura terminar de copiar a saída final.
                std::thread::sleep(Duration::from_millis(100));
                return st.code().unwrap_or(-1);
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!("o comando não terminou; tela:\n{}", self.screen());
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    /// Fecha o terminal (o mestre do PTY), como fechar a janela.
    pub fn close(self) -> Child {
        self.master.lock().unwrap().take();
        self.child
    }
}

/// Respostas às consultas encontradas em `out`.
/// Só conta consultas que terminam depois de `boundary` (os bytes novos).
fn answers(out: &[u8], boundary: usize, screen: &vt100::Screen) -> Vec<u8> {
    let has = |q: &[u8]| {
        out.windows(q.len())
            .enumerate()
            .any(|(i, w)| w == q && i + q.len() > boundary)
    };
    let mut r = Vec::new();
    if has(b"\x1b[c") || has(b"\x1b[0c") {
        r.extend_from_slice(b"\x1b[?62;22c");
    }
    if has(b"\x1b[>c") || has(b"\x1b[>0c") {
        r.extend_from_slice(b"\x1b[>1;10;0c");
    }
    if has(b"\x1b[6n") {
        let (row, col) = screen.cursor_position();
        r.extend_from_slice(format!("\x1b[{};{}R", row + 1, col + 1).as_bytes());
    }
    if has(b"\x1b[?u") {
        r.extend_from_slice(b"\x1b[?0u");
    }
    if has(b"\x1b]10;?") {
        r.extend_from_slice(b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\");
    }
    if has(b"\x1b]11;?") {
        r.extend_from_slice(b"\x1b]11;rgb:0000/0000/0000\x1b\\");
    }
    if has(b"\x1b[?2026$p") {
        r.extend_from_slice(b"\x1b[?2026;2$y");
    }
    r
}
