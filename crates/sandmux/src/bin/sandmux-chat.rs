//! Programa da aba de chat. Roda dentro do sandbox, na pasta da aba, com a
//! conversa em `conversa.json` e os provedores em `SANDMUX_CHAT_PROVIDERS`.
//!
//! `sandmux-chat` desenha a conversa, a barra de texto e o botão "Enviar";
//! `sandmux-chat --once` responde a pergunta pendente na saída padrão.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use sandmux::chat::{self, Conversa, Mensagem, Papel};
use sandmux_policy::assistant::Provider;
use unicode_width::UnicodeWidthChar;

fn main() {
    let code = match run() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sandmux-chat: {e:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32> {
    let mut once = false;
    let mut dir = std::env::current_dir().context("pasta atual")?;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--once" => once = true,
            "--dir" => dir = PathBuf::from(args.next().ok_or_else(|| anyhow!("falta a pasta"))?),
            other => return Err(anyhow!("argumento desconhecido: {other}")),
        }
    }
    let conv = Conversa::load(&dir)?
        .ok_or_else(|| anyhow!("não há conversa em {}", dir.join(chat::FILE).display()))?;
    let providers = chat::providers_from_env();
    if once {
        return Ok(answer_once(&conv, &providers));
    }
    ui(dir, conv, providers)
}

/// Responde a pergunta pendente, sem streaming, na saída padrão.
fn answer_once(conv: &Conversa, providers: &[Provider]) -> i32 {
    if let Err(e) = chat::ready(conv) {
        eprintln!("sandmux-chat: {e}");
        return 1;
    }
    let t = match chat::target(&conv.modelo, providers) {
        Ok(t) => chat::Target {
            system: conv.sistema.clone(),
            ..t
        },
        Err(e) => {
            eprintln!("sandmux-chat: {e}");
            return 1;
        }
    };
    match chat::complete_blocking(
        &t,
        &conv.mensagens,
        false,
        &AtomicBool::new(false),
        &mut |_| {},
    ) {
        Ok(text) => {
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(text.as_bytes());
            if !text.ends_with('\n') {
                let _ = out.write_all(b"\n");
            }
            let _ = out.flush();
            0
        }
        Err(e) => {
            eprintln!("sandmux-chat: {e}");
            1
        }
    }
}

enum Ev {
    Delta(String),
    Done(Result<String, String>),
}

struct App {
    dir: PathBuf,
    conv: Conversa,
    providers: Vec<Provider>,
    input: String,
    /// Resposta em curso: o cancelamento e o texto já chegado.
    busy: Option<Arc<AtomicBool>>,
    partial: String,
    /// Avisos e erros, mostrados depois da conversa até o próximo envio.
    notes: Vec<String>,
    /// Linhas roladas a partir do fim.
    scroll: usize,
    /// Altura da conversa no último desenho.
    view: usize,
    button: Rect,
    tx: Sender<Ev>,
}

impl App {
    fn save(&mut self) {
        if let Err(e) = self.conv.save(&self.dir) {
            self.notes
                .push(format!("não foi possível gravar a conversa: {e:#}"));
        }
    }

    fn start(&mut self) {
        if self.busy.is_some() || !self.conv.pending() {
            return;
        }
        let t = match chat::target(&self.conv.modelo, &self.providers) {
            Ok(t) => chat::Target {
                system: self.conv.sistema.clone(),
                ..t
            },
            Err(e) => {
                self.notes.push(e);
                return;
            }
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.busy = Some(cancel.clone());
        self.partial.clear();
        let msgs = self.conv.mensagens.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let d = tx.clone();
            let r = chat::complete_blocking(&t, &msgs, true, &cancel, &mut |s| {
                let _ = d.send(Ev::Delta(s.to_string()));
            });
            let _ = tx.send(Ev::Done(r));
        });
    }

    fn on(&mut self, ev: Ev) {
        match ev {
            Ev::Delta(s) => self.partial.push_str(&s),
            Ev::Done(r) => {
                self.busy = None;
                let partial = std::mem::take(&mut self.partial);
                match r {
                    Ok(text) => {
                        let text = if text.is_empty() { partial } else { text };
                        self.conv.mensagens.push(Mensagem {
                            papel: Papel::Assistant,
                            texto: text,
                        });
                        self.save();
                    }
                    Err(e) => self.notes.push(format!("erro: {e}")),
                }
            }
        }
    }

    fn command(&mut self, text: &str) {
        let arg = text.trim_start_matches("/modelo").trim();
        if arg.is_empty() {
            self.notes.push(format!(
                "modelo atual: {}. Provedores: {}",
                self.conv.modelo,
                self.providers
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            return;
        }
        match chat::target(arg, &self.providers) {
            Ok(_) => {
                self.conv.modelo = arg.to_string();
                self.save();
                self.notes.push(format!("modelo trocado para {arg}"));
            }
            Err(e) => self.notes.push(e),
        }
    }

    fn send(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.notes.clear();
        if text == "/modelo" || text.starts_with("/modelo ") {
            self.input.clear();
            self.command(&text);
            return;
        }
        if self.busy.is_some() {
            self.notes
                .push("aguarde a resposta (Ctrl+c cancela)".into());
            return;
        }
        self.input.clear();
        self.scroll = 0;
        self.conv.mensagens.push(Mensagem {
            papel: Papel::User,
            texto: text,
        });
        self.save();
        self.start();
    }

    fn cancel(&mut self) {
        if let Some(c) = &self.busy {
            c.store(true, Ordering::Relaxed);
        }
    }
}

/// Quebra um texto pela largura, preferindo os espaços.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for raw in text.split('\n') {
        let mut line = String::new();
        let mut w = 0;
        // Início da última palavra na linha atual (posição em bytes e largura
        // até lá).
        let mut last_space: Option<(usize, usize)> = None;
        for ch in raw.chars() {
            let ch = if ch == '\t' { ' ' } else { ch };
            let cw = ch.width().unwrap_or(0);
            if w + cw > width && !line.is_empty() {
                if ch == ' ' {
                    // O espaço que não cabe vira a quebra.
                    out.push(std::mem::take(&mut line));
                    w = 0;
                    last_space = None;
                    continue;
                }
                match last_space {
                    Some((i, sw)) if i > 0 => {
                        let rest = line[i..].to_string();
                        line.truncate(i);
                        out.push(line.trim_end().to_string());
                        line = rest;
                        w -= sw;
                    }
                    _ => {
                        out.push(std::mem::take(&mut line));
                        w = 0;
                    }
                }
                last_space = None;
            }
            line.push(ch);
            w += cw;
            if ch == ' ' {
                last_space = Some((line.len(), w));
            }
        }
        out.push(line);
    }
    out
}

fn lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let you = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let model = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let note = Style::default().fg(Color::Yellow);
    let mut out: Vec<Line<'static>> = Vec::new();
    let push = |out: &mut Vec<Line<'static>>, who: &str, st: Style, text: &str| {
        out.push(Line::from(Span::styled(who.to_string(), st)));
        for l in wrap(text, width) {
            out.push(Line::from(l));
        }
        out.push(Line::from(""));
    };
    for m in &app.conv.mensagens {
        match m.papel {
            Papel::User => push(&mut out, "você", you, &m.texto),
            Papel::Assistant => push(&mut out, "modelo", model, &m.texto),
        }
    }
    if app.busy.is_some() {
        push(&mut out, "modelo", model, &format!("{}▌", app.partial));
    }
    for n in &app.notes {
        for l in wrap(&format!("! {n}"), width) {
            out.push(Line::from(Span::styled(l, note)));
        }
    }
    out
}

fn draw(f: &mut ratatui::Frame, app: &mut App) {
    let area = f.area();
    let input_w = area.width.saturating_sub(14).max(4) as usize;
    let input_lines = wrap(&app.input, input_w.saturating_sub(2).max(1));
    let ih = (input_lines.len().clamp(1, 5) + 2) as u16;
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(ih),
    ])
    .areas(area);
    let status = if app.busy.is_some() {
        "  respondendo… (Ctrl+c cancela)"
    } else {
        ""
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" chat ", Style::default().add_modifier(Modifier::REVERSED)),
            Span::raw(format!(" {}{status}", app.conv.modelo)),
        ])),
        top,
    );
    let all = lines(app, body.width.saturating_sub(1).max(1) as usize);
    let h = body.height as usize;
    app.view = h;
    let max_scroll = all.len().saturating_sub(h);
    app.scroll = app.scroll.min(max_scroll);
    let end = all.len() - app.scroll;
    let start = end.saturating_sub(h);
    f.render_widget(Paragraph::new(all[start..end].to_vec()), body);
    let [inp, btn] = Layout::horizontal([Constraint::Min(4), Constraint::Length(12)]).areas(bottom);
    let shown: Vec<Line> = input_lines
        .iter()
        .skip(input_lines.len().saturating_sub(5))
        .map(|l| Line::from(l.clone()))
        .collect();
    let last = shown.last().map(|l| l.width()).unwrap_or(0) as u16;
    let nshown = shown.len() as u16;
    f.render_widget(
        Paragraph::new(shown).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" mensagem (Enter envia, Ctrl+j quebra a linha, /modelo troca) "),
        ),
        inp,
    );
    let by = btn.y + btn.height.saturating_sub(1) / 2;
    app.button = Rect::new(btn.x + 1, by, 10, 1);
    let bstyle = if app.busy.is_some() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
    };
    f.render_widget(
        Paragraph::new(Span::styled("[ Enviar ]", bstyle)),
        app.button,
    );
    f.set_cursor_position(Position::new(
        (inp.x + 1 + last).min(inp.right().saturating_sub(2)),
        inp.y + nshown,
    ));
}

struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = ratatui::crossterm::execute!(
            std::io::stdout(),
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
    }
}

fn ui(dir: PathBuf, conv: Conversa, providers: Vec<Provider>) -> Result<i32> {
    let (tx, rx): (Sender<Ev>, Receiver<Ev>) = mpsc::channel();
    enable_raw_mode().context("modo cru do terminal")?;
    let _guard = Guard;
    ratatui::crossterm::execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut term = ratatui::Terminal::new(backend)?;
    let mut app = App {
        dir,
        conv,
        providers,
        input: String::new(),
        busy: None,
        partial: String::new(),
        notes: Vec::new(),
        scroll: 0,
        view: 1,
        button: Rect::default(),
        tx,
    };
    // Pergunta sem resposta: a primeira do modal, ou uma interrompida.
    app.start();
    loop {
        term.draw(|f| draw(f, &mut app))?;
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(k) if k.kind != KeyEventKind::Release => {
                    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
                    let alt = k.modifiers.contains(KeyModifiers::ALT);
                    match k.code {
                        KeyCode::Enter if alt => app.input.push('\n'),
                        KeyCode::Enter => app.send(),
                        KeyCode::Char('j') if ctrl => app.input.push('\n'),
                        KeyCode::Char('c') if ctrl => app.cancel(),
                        KeyCode::Char('u') if ctrl => app.input.clear(),
                        KeyCode::Char(c) if !ctrl => app.input.push(c),
                        KeyCode::Backspace => {
                            app.input.pop();
                        }
                        KeyCode::PageUp => app.scroll += app.view.saturating_sub(2).max(1),
                        KeyCode::PageDown => {
                            app.scroll =
                                app.scroll.saturating_sub(app.view.saturating_sub(2).max(1))
                        }
                        _ => {}
                    }
                }
                Event::Paste(s) => app
                    .input
                    .push_str(&s.replace("\r\n", "\n").replace('\r', "\n")),
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::ScrollUp => app.scroll += 3,
                    MouseEventKind::ScrollDown => app.scroll = app.scroll.saturating_sub(3),
                    MouseEventKind::Down(MouseButton::Left)
                        if app.button.contains(Position::new(m.column, m.row)) =>
                    {
                        app.send()
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        while let Ok(ev) = rx.try_recv() {
            app.on(ev);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn quebra_pelos_espacos() {
        assert_eq!(wrap("abc def ghi", 7), vec!["abc def", "ghi"]);
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap("a\n\nb", 10), vec!["a", "", "b"]);
        // Caracteres largos ocupam duas colunas.
        assert_eq!(wrap("日本語です", 4), vec!["日本", "語で", "す"]);
    }
}
