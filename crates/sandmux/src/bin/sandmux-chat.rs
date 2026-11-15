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
use sandmux_spec::{tr, trf};
use unicode_width::UnicodeWidthChar;

fn main() {
    sandmux_spec::lang::set(sandmux_spec::lang::resolve_env(None));
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
    let mut models = false;
    let mut dir = std::env::current_dir().context(tr!("pasta atual", "current folder"))?;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--once" => once = true,
            "--models" => models = true,
            "--dir" => {
                dir = PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow!(tr!("falta a pasta", "missing folder")))?,
                )
            }
            other => {
                return Err(anyhow!(trf!(
                    "argumento desconhecido: {other}",
                    "unknown argument: {other}"
                )));
            }
        }
    }
    if models {
        return Ok(print_models());
    }
    let conv = Conversa::load(&dir)?.ok_or_else(|| {
        anyhow!(trf!(
            "não há conversa em {}",
            "there is no conversation in {}",
            dir.join(chat::FILE).display()
        ))
    })?;
    let providers = chat::providers_from_env();
    if once {
        return Ok(answer_once(&conv, &providers));
    }
    ui(dir, conv, providers)
}

/// Imprime os modelos dos provedores, um `provedor/modelo` por linha; os
/// provedores que falharam vão para a saída de erro. Falha só sem nenhum.
fn print_models() -> i32 {
    let (ids, notes) = chat::all_models(&chat::providers_from_env());
    for n in &notes {
        eprintln!("sandmux-chat: {n}");
    }
    if ids.is_empty() {
        return 1;
    }
    let mut out = std::io::stdout().lock();
    for id in ids {
        let _ = writeln!(out, "{id}");
    }
    let _ = out.flush();
    0
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
    /// A lista de modelos dos provedores e os avisos.
    Models(Vec<String>, Vec<String>),
}

/// O seletor de `/modelo`: o filtro e o escolhido.
struct Picker {
    filter: String,
    sel: usize,
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
    picker: Option<Picker>,
    /// A lista de modelos (`None` enquanto carrega) e os avisos.
    models: Option<(Vec<String>, Vec<String>)>,
    loading: bool,
    /// Começo da resposta em curso, para o tempo no cartão.
    started: Option<std::time::Instant>,
    /// As cores do tema ativo.
    pal: Pal,
}

/// As cores do tema ativo, de `SANDMUX_THEME`, com as da TUI como recuo.
struct Pal {
    bg: Color,
    fg: Color,
    muted: Color,
    accent: Color,
    user: Color,
    chip_fg: Color,
    idx_bg: Color,
    idx_fg: Color,
    err: Color,
}

impl Pal {
    fn from_env() -> Pal {
        use sandmux_policy::config::ThemeColor;
        let map: std::collections::BTreeMap<String, String> = std::env::var(chat::THEME_ENV)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let c = |el: &str, padrao: &str| {
            let tc = map
                .get(el)
                .and_then(|v| ThemeColor::parse(v))
                .or_else(|| ThemeColor::parse(padrao));
            match tc {
                Some(ThemeColor::Rgb(r, g, b)) => Color::Rgb(r, g, b),
                Some(ThemeColor::Indexed(i)) => Color::Indexed(i),
                _ => Color::Reset,
            }
        };
        Pal {
            bg: c("page-bg", "#1e1e2e"),
            fg: c("page-fg", "#cdd6f4"),
            muted: c("muted-fg", "#6c7086"),
            accent: c("accent", "#cba6f7"),
            user: c("chip-harness-bg", "#f38ba8"),
            chip_fg: c("chip-fg", "#1e1e2e"),
            idx_bg: c("tab-index-bg", "#45475a"),
            idx_fg: c("tab-index-fg", "#cdd6f4"),
            err: c("error-fg", "#f38ba8"),
        }
    }
}

impl App {
    fn save(&mut self) {
        if let Err(e) = self.conv.save(&self.dir) {
            self.notes.push(trf!(
                "não foi possível gravar a conversa: {e:#}",
                "could not save the conversation: {e:#}"
            ));
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
        self.started = Some(std::time::Instant::now());
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
            Ev::Models(ids, notes) => {
                self.loading = false;
                self.models = Some((ids, notes));
            }
            Ev::Done(r) => {
                self.busy = None;
                let partial = std::mem::take(&mut self.partial);
                match r {
                    Ok(text) => {
                        let text = if text.is_empty() { partial } else { text };
                        let ms = self.started.take().map(|s| s.elapsed().as_millis() as u64);
                        self.conv.mensagens.push(Mensagem {
                            papel: Papel::Assistant,
                            texto: text,
                            ms,
                        });
                        self.save();
                    }
                    Err(e) => self.notes.push(trf!("erro: {e}", "error: {e}")),
                }
            }
        }
    }

    fn command(&mut self, text: &str) {
        let arg = text.trim_start_matches("/modelo").trim();
        if arg.is_empty() {
            // Sem argumento: o seletor com a lista dos provedores.
            self.picker = Some(Picker {
                filter: String::new(),
                sel: 0,
            });
            if self.models.is_none() {
                self.load_models();
            }
            return;
        }
        match chat::target(arg, &self.providers) {
            Ok(_) => {
                self.conv.modelo = arg.to_string();
                self.save();
                self.notes
                    .push(trf!("modelo trocado para {arg}", "model switched to {arg}"));
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
            self.notes.push(
                tr!(
                    "aguarde a resposta (Ctrl+c cancela)",
                    "wait for the answer (Ctrl+c cancels)"
                )
                .into(),
            );
            return;
        }
        self.input.clear();
        self.scroll = 0;
        self.conv.mensagens.push(Mensagem {
            papel: Papel::User,
            texto: text,
            ms: None,
        });
        self.save();
        self.start();
    }

    /// Pede a lista de modelos numa thread.
    fn load_models(&mut self) {
        if self.loading {
            return;
        }
        self.loading = true;
        self.models = None;
        let providers = self.providers.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let (ids, notes) = chat::all_models(&providers);
            let _ = tx.send(Ev::Models(ids, notes));
        });
    }

    /// Os modelos que passam no filtro do seletor.
    fn picked_list(&self) -> Vec<String> {
        match (&self.picker, &self.models) {
            (Some(p), Some((ids, _))) => chat::filter_models(ids, &p.filter)
                .into_iter()
                .cloned()
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Uma tecla com o seletor aberto.
    fn picker_key(&mut self, code: KeyCode, ctrl: bool) {
        let list = self.picked_list();
        let Some(p) = self.picker.as_mut() else {
            return;
        };
        match code {
            KeyCode::Esc => self.picker = None,
            KeyCode::Up => p.sel = p.sel.saturating_sub(1),
            KeyCode::Down => p.sel = (p.sel + 1).min(list.len().saturating_sub(1)),
            KeyCode::Backspace => {
                p.filter.pop();
                p.sel = 0;
            }
            KeyCode::Char('r') if ctrl => self.load_models(),
            KeyCode::Char(c) if !ctrl => {
                p.filter.push(c);
                p.sel = 0;
            }
            KeyCode::Enter => {
                let chosen = list
                    .get(p.sel)
                    .cloned()
                    .unwrap_or_else(|| p.filter.trim().to_string());
                self.picker = None;
                if !chosen.is_empty() {
                    self.command(&format!("/modelo {chosen}"));
                }
            }
            _ => {}
        }
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

/// O tempo de resposta, em segundos, com a vírgula no português.
fn secs(ms: u64) -> String {
    let s = format!("{:.1} s", ms as f64 / 1000.0);
    if sandmux_spec::lang::is_en() {
        s
    } else {
        s.replace('.', ",")
    }
}

/// Um cartão de borda arredondada: o papel no título, um texto à direita
/// do título (o tempo) e o texto quebrado na largura.
fn card(
    out: &mut Vec<Line<'static>>,
    width: usize,
    title: &str,
    right: Option<String>,
    border: Color,
    fg: Color,
    text: &str,
) {
    use unicode_width::UnicodeWidthStr;
    let w = width.max(8);
    let b = Style::default().fg(border);
    let left = format!("╭ {title} ");
    let right = match right {
        Some(r) => format!(" {r} ─╮"),
        None => "─╮".to_string(),
    };
    let fill = w.saturating_sub(left.width() + right.width());
    out.push(Line::from(vec![
        Span::styled("╭ ", b),
        Span::styled(title.to_string(), b.add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {}", "─".repeat(fill)), b),
        Span::styled(right, b),
    ]));
    let inner = w.saturating_sub(4).max(1);
    for l in wrap(text, inner) {
        let pad = inner.saturating_sub(l.width());
        out.push(Line::from(vec![
            Span::styled("│ ", b),
            Span::styled(format!("{l}{}", " ".repeat(pad)), Style::default().fg(fg)),
            Span::styled(" │", b),
        ]));
    }
    out.push(Line::from(Span::styled(
        format!("╰{}╯", "─".repeat(w.saturating_sub(2))),
        b,
    )));
}

fn lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let p = &app.pal;
    let mut out: Vec<Line<'static>> = Vec::new();
    for m in &app.conv.mensagens {
        match m.papel {
            Papel::User => card(
                &mut out,
                width,
                tr!("você", "you"),
                None,
                p.user,
                p.fg,
                &m.texto,
            ),
            Papel::Assistant => card(
                &mut out,
                width,
                tr!("modelo", "model"),
                m.ms.map(secs),
                p.accent,
                p.fg,
                &m.texto,
            ),
        }
    }
    if app.busy.is_some() {
        card(
            &mut out,
            width,
            tr!("modelo", "model"),
            Some("…".into()),
            p.accent,
            p.fg,
            &format!("{}▌", app.partial),
        );
    }
    for n in &app.notes {
        for l in wrap(&format!("! {n}"), width) {
            out.push(Line::from(Span::styled(l, Style::default().fg(p.err))));
        }
    }
    out
}

fn draw(f: &mut ratatui::Frame, app: &mut App) {
    let area = f.area();
    let p = &app.pal;
    let (bg, fg, muted, accent) = (p.bg, p.fg, p.muted, p.accent);
    let (chip_fg, idx_bg, idx_fg) = (p.chip_fg, p.idx_bg, p.idx_fg);
    // O fundo e o texto da página em toda a aba.
    f.render_widget(Block::default().style(Style::default().bg(bg).fg(fg)), area);
    let input_w = area.width.saturating_sub(16).max(4) as usize;
    let input_lines = wrap(&app.input, input_w.max(1));
    let ih = (input_lines.len().clamp(1, 5) + 2) as u16;
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(ih),
    ])
    .areas(area);
    let status = if app.busy.is_some() {
        tr!(
            "  respondendo… (Ctrl+c cancela)",
            "  answering… (Ctrl+c cancels)"
        )
    } else {
        ""
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " chat",
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" › ", Style::default().fg(muted)),
            Span::styled(app.conv.modelo.clone(), Style::default().fg(fg)),
            Span::styled(
                status,
                Style::default().fg(muted).add_modifier(Modifier::ITALIC),
            ),
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
    // O campo da mensagem num cartão, com o botão em dois tons.
    let prefix = std::env::var("SANDMUX_PREFIX").unwrap_or_else(|_| "Ctrl+b".into());
    let mbox = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(muted))
        .title(Span::styled(
            tr!(" mensagem ", " message "),
            Style::default().fg(muted),
        ))
        .title_bottom(Span::styled(
            trf!(
                " Enter envia · Ctrl+j quebra · /modelo troca · {prefix} d sai ",
                " Enter sends · Ctrl+j breaks · /modelo switches · {prefix} d leaves "
            ),
            Style::default().fg(muted),
        ));
    let inner = mbox.inner(bottom);
    f.render_widget(mbox, bottom);
    let [inp, btn] = Layout::horizontal([Constraint::Min(4), Constraint::Length(13)]).areas(inner);
    let shown: Vec<Line> = input_lines
        .iter()
        .skip(input_lines.len().saturating_sub(5))
        .map(|l| Line::from(l.clone()))
        .collect();
    let last = shown.last().map(|l| l.width()).unwrap_or(0) as u16;
    let nshown = shown.len() as u16;
    f.render_widget(Paragraph::new(shown), inp);
    let label = tr!(" Enviar ", " Send ");
    let bw = 3 + label.chars().count() as u16;
    let by = btn.y + btn.height.saturating_sub(1);
    app.button = Rect::new(btn.right().saturating_sub(bw), by, bw, 1);
    let (icon, text) = if app.busy.is_some() {
        (
            Style::default().fg(muted).bg(idx_bg),
            Style::default().fg(muted).bg(idx_bg),
        )
    } else {
        (
            Style::default().fg(idx_fg).bg(idx_bg),
            Style::default()
                .fg(chip_fg)
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        )
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ⏎ ", icon),
            Span::styled(label, text),
        ])),
        app.button,
    );
    f.set_cursor_position(Position::new(
        (inp.x + last).min(inp.right().saturating_sub(1)),
        inp.y + nshown.saturating_sub(1),
    ));
    if let Some(p) = &app.picker {
        draw_picker(f, body, app, p);
    }
}

/// O seletor de `/modelo` por cima da conversa.
fn draw_picker(f: &mut ratatui::Frame, body: Rect, app: &App, p: &Picker) {
    let w = body.width.saturating_sub(4).min(84);
    let h = body.height.saturating_sub(2).min(24);
    if w < 20 || h < 6 {
        return;
    }
    let r = Rect::new(
        body.x + (body.width - w) / 2,
        body.y + (body.height - h) / 2,
        w,
        h,
    );
    let pal = &app.pal;
    let list = app.picked_list();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(tr!("filtro: ", "filter: "), Style::default().fg(pal.muted)),
            Span::styled(
                format!("{}▏", p.filter),
                Style::default().fg(pal.fg).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::styled(
            match &app.models {
                None => tr!(
                    "carregando os modelos dos provedores…",
                    "loading the providers' models…"
                )
                .to_string(),
                Some((ids, _)) if list.is_empty() => trf!(
                    "nenhum de {} modelos; Enter usa o texto digitado",
                    "none of {} models; Enter uses the typed text",
                    ids.len()
                ),
                Some((ids, _)) => trf!("{} de {}", "{} of {}", list.len(), ids.len()),
            },
            Style::default().fg(pal.muted),
        ),
    ];
    if let Some((_, notes)) = &app.models {
        for n in notes.iter().take(2) {
            lines.push(Line::styled(format!("! {n}"), Style::default().fg(pal.err)));
        }
    }
    let room = (h as usize).saturating_sub(lines.len() + 2);
    let sel = p.sel.min(list.len().saturating_sub(1));
    let start = sel.saturating_sub(room.saturating_sub(1));
    for (i, m) in list.iter().enumerate().skip(start).take(room) {
        lines.push(if i == sel {
            Line::styled(
                format!(" {m} "),
                Style::default()
                    .fg(pal.chip_fg)
                    .bg(pal.accent)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Line::styled(format!(" {m} "), Style::default().fg(pal.fg))
        });
    }
    f.render_widget(ratatui::widgets::Clear, r);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .border_style(Style::default().fg(pal.accent))
                .style(Style::default().bg(pal.bg).fg(pal.fg))
                .title(trf!(" modelo: {} ", " model: {} ", app.conv.modelo))
                .title_bottom(tr!(
                    " ↑ ↓ · Enter escolhe · Ctrl+r relê · Esc cancela ",
                    " ↑ ↓ · Enter picks · Ctrl+r reloads · Esc cancels "
                )),
        ),
        r,
    );
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
    enable_raw_mode().context(tr!("modo cru do terminal", "terminal raw mode"))?;
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
        picker: None,
        models: None,
        loading: false,
        started: None,
        pal: Pal::from_env(),
    };
    // Pergunta sem resposta: a primeira do modal, ou uma interrompida.
    app.start();
    loop {
        term.draw(|f| draw(f, &mut app))?;
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(k) if k.kind != KeyEventKind::Release && app.picker.is_some() => {
                    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
                    app.picker_key(k.code, ctrl);
                }
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
