//! Desenho: barra de abas, tela da aba ativa e modais.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use super::{App, Modal, Sel};
use sandmux_policy::config::{Theme, ThemeColor};
use sandmux_policy::limits::human;
use sandmux_proto::{Msg, SessionState};

fn tcolor(c: ThemeColor) -> Color {
    match c {
        ThemeColor::Default => Color::Reset,
        ThemeColor::Indexed(i) => Color::Indexed(i),
        ThemeColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Estilo de um elemento: as cores do tema, quando ele define alguma; senão
/// o estilo do tema `padrao`.
pub fn themed(theme: &Theme, fg: &str, bg: &str, padrao: Style) -> Style {
    match (theme.get(fg), theme.get(bg)) {
        (None, None) => padrao,
        (f, b) => {
            let mut s = Style::default();
            if let Some(f) = f {
                s = s.fg(tcolor(f));
            }
            if let Some(b) = b {
                s = s.bg(tcolor(b));
            }
            s
        }
    }
}

/// Cor do `vt100` para o `ratatui`.
pub fn color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Estilo de uma célula do `vt100`.
pub fn cell_style(c: &vt100::Cell) -> Style {
    let mut s = Style::default()
        .fg(color(c.fgcolor()))
        .bg(color(c.bgcolor()));
    let mut m = Modifier::empty();
    if c.bold() {
        m |= Modifier::BOLD;
    }
    if c.dim() {
        m |= Modifier::DIM;
    }
    if c.italic() {
        m |= Modifier::ITALIC;
    }
    if c.underline() {
        m |= Modifier::UNDERLINED;
    }
    if c.inverse() {
        m |= Modifier::REVERSED;
    }
    s = s.add_modifier(m);
    s
}

/// Copia a tela do `vt100` para a área do buffer, com a seleção invertida.
pub fn draw_screen(
    buf: &mut Buffer,
    area: Rect,
    screen: &vt100::Screen,
    sel: Option<&Sel>,
    sel_style: Style,
) {
    let (rows, cols) = screen.size();
    for row in 0..area.height.min(rows.get()) {
        for col in 0..area.width.min(cols.get()) {
            let Some(c) = screen.cell(row, col) else {
                continue;
            };
            let Some(dst) = buf.cell_mut(Position::new(area.x + col, area.y + row)) else {
                continue;
            };
            if c.is_wide_continuation() {
                dst.set_symbol("");
                continue;
            }
            let text = c.contents();
            dst.set_symbol(if text.is_empty() { " " } else { text });
            let mut st = cell_style(c);
            if sel.is_some_and(|s| s.contains(row, col)) {
                st = st.patch(sel_style);
            }
            dst.set_style(st);
        }
    }
}

fn tab_label(i: usize, t: &super::Tab) -> String {
    let mark = match &t.state {
        SessionState::Viva => "",
        SessionState::Encerrada { .. } => " ✕",
        SessionState::Parada { .. } => " ‖",
        SessionState::AguardandoCofre => " (cofre)",
        SessionState::Reiniciando { .. } => " ↻",
    };
    let net = match (t.blocked, t.new_domain) {
        (true, true) => " !•",
        (true, false) => " !",
        (false, true) => " •",
        _ => "",
    };
    format!(" {}:{}{}{} ", i + 1, t.name, mark, net)
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let full = f.area();
    let bar = Rect::new(full.x, full.y, full.width, 1);
    let panel_on = super::panel_visible(app.panel, full.width);
    let panel_w = if panel_on { super::PANEL } else { 0 };
    let area = Rect::new(
        full.x,
        full.y + 1,
        full.width - panel_w,
        full.height.saturating_sub(1),
    );
    if panel_on {
        let r = Rect::new(
            full.x + full.width - panel_w,
            full.y + 1,
            panel_w,
            full.height.saturating_sub(1),
        );
        draw_panel(f, r, app);
    }

    // Barra de abas.
    let theme = app.cfg.theme.clone();
    let mut spans = Vec::new();
    app.bar_ranges.clear();
    let mut x = 0u16;
    for (i, t) in app.tabs.iter().enumerate() {
        let label = tab_label(i, t);
        let w = label.chars().count() as u16;
        let style = if i == app.active {
            themed(
                &theme,
                "tab-active-fg",
                "tab-active-bg",
                Style::default().add_modifier(Modifier::REVERSED),
            )
            .add_modifier(Modifier::BOLD)
        } else {
            themed(
                &theme,
                "tab-fg",
                "tab-bg",
                Style::default().add_modifier(Modifier::DIM),
            )
        };
        app.bar_ranges.push((x, x + w, i));
        x += w;
        spans.push(Span::styled(label, style));
    }
    let status = app.status_text();
    let used: u16 = x;
    let free = bar.width.saturating_sub(used);
    if !status.is_empty() && free > 2 {
        let s: String = status.chars().take(free as usize - 1).collect();
        let pad = free as usize - s.chars().count();
        spans.push(Span::raw(" ".repeat(pad.saturating_sub(1))));
        spans.push(Span::styled(
            s,
            themed(&theme, "status-fg", "", Style::default()).add_modifier(Modifier::ITALIC),
        ));
    }
    let bar_style = themed(&theme, "", "bar-bg", Style::default());
    f.render_widget(Paragraph::new(Line::from(spans)).style(bar_style), bar);

    // Tela da aba ativa.
    let modal_open = app.modal.is_some();
    if let Some(t) = app.tabs.get(app.active) {
        let screen = t.parser.screen();
        let sel_style = themed(
            &theme,
            "selection-fg",
            "selection-bg",
            Style::default().add_modifier(Modifier::REVERSED),
        );
        draw_screen(f.buffer_mut(), area, screen, app.sel.as_ref(), sel_style);
        if !modal_open && t.scroll == 0 && !screen.hide_cursor() && t.state == SessionState::Viva {
            let (r, c) = screen.cursor_position();
            f.set_cursor_position(Position::new(area.x + c, area.y + r));
        }
        let note = match &t.state {
            SessionState::Encerrada { codigo } => Some(format!(
                " encerrada ({codigo}) · {} r reinicia · {} x fecha ",
                app.prefix_name(),
                app.prefix_name()
            )),
            SessionState::Parada { motivo } => Some(format!(
                " parada: {motivo} · {} r reinicia · {} x fecha ",
                app.prefix_name(),
                app.prefix_name()
            )),
            SessionState::Viva if t.scroll > 0 => {
                Some(format!(" histórico: {} linhas acima ", t.scroll))
            }
            SessionState::Viva => None,
            SessionState::AguardandoCofre => Some(
                " aguardando cofre: a sessão sobe quando o cofre for desbloqueado (sandmux vault unlock) "
                    .to_string(),
            ),
            SessionState::Reiniciando { segundos } => Some(format!(
                " o gateway terminou e volta em {segundos} s · {} x fecha ",
                app.prefix_name()
            )),
        };
        if let Some(n) = note
            && area.height > 0
        {
            let r = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
            f.render_widget(Clear, r);
            f.render_widget(
                Paragraph::new(n).style(themed(
                    &theme,
                    "note-fg",
                    "note-bg",
                    Style::default().add_modifier(Modifier::REVERSED),
                )),
                r,
            );
        }
    }

    if let Some(m) = &app.modal {
        draw_modal(f, full, m, app);
    }
}

/// Painel lateral: proteção, recursos e rede da aba ativa.
fn draw_panel(f: &mut Frame, r: Rect, app: &App) {
    let theme = &app.cfg.theme;
    let head = themed(theme, "modal-title", "", Style::default()).add_modifier(Modifier::BOLD);
    let home = std::env::var("HOME").unwrap_or_default();
    // O HOME aparece como `~`; as linhas longas quebram.
    let short = |s: String| {
        if home.is_empty() {
            s
        } else {
            s.replace(&home, "~")
        }
    };
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::styled("Proteção", head));
    match &app.panel_data {
        Some(Msg::StatsOf {
            state,
            stats,
            oom_kills,
            docker,
            policy,
            ports,
            ..
        }) => {
            match policy {
                Some(p) => {
                    for l in crate::sessions::policy_lines(p) {
                        lines.push(Line::raw(short(l)));
                    }
                }
                None => lines.push(Line::raw(short("sem lançamento registrado".into()))),
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled("Recursos", head));
            lines.push(Line::raw(short(format!("estado: {state}"))));
            match stats {
                Some(s) => {
                    let teto = |m: Option<u64>, f: &dyn Fn(u64) -> String| {
                        m.map(f).unwrap_or_else(|| "sem teto".into())
                    };
                    lines.push(Line::raw(short(format!(
                        "memória: {} / {}",
                        human(s.usage.memory),
                        teto(s.usage.memory_max, &human)
                    ))));
                    lines.push(Line::raw(short(format!("pico: {}", human(s.memory_peak)))));
                    lines.push(Line::raw(short(format!(
                        "processos: {} / {}",
                        s.usage.processes,
                        teto(s.usage.processes_max, &|n| n.to_string())
                    ))));
                    lines.push(Line::raw(short(format!(
                        "CPU: {}s",
                        format!("{:.1}", s.cpu_usec as f64 / 1e6).replace('.', ",")
                    ))));
                }
                None => lines.push(Line::raw(short("sem consumo medido".into()))),
            }
            lines.push(Line::raw(short(format!(
                "mortes por falta de memória: {oom_kills}"
            ))));
            match docker {
                Some(d) => {
                    lines.push(Line::raw(short(format!(
                        "Docker: {} contêineres",
                        d.containers
                    ))));
                    lines.push(Line::raw(short(format!("  memória: {}", human(d.memory)))));
                }
                None => lines.push(Line::raw(short("Docker: parado".into()))),
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled("Portas", head));
            if ports.is_empty() {
                lines.push(Line::raw("nenhuma publicada"));
            }
            for l in crate::sessions::port_lines(ports) {
                lines.push(Line::raw(l));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled("Rede", head));
            if let Some(p) = policy {
                lines.push(Line::raw(format!("modo: {}", p.network)));
            }
            if app.panel_net.is_empty() {
                lines.push(Line::raw("sem conexões registradas"));
            }
            let novo =
                themed(theme, "status-fg", "", Style::default()).add_modifier(Modifier::BOLD);
            for l in &app.panel_net {
                let what = match (&l.method, &l.path) {
                    (Some(m), Some(p)) => format!(
                        "{} {m} {p} {}",
                        l.domain,
                        l.status.map(|s| s.to_string()).unwrap_or_default()
                    ),
                    _ => l.domain.clone(),
                };
                let mut spans = vec![Span::raw(format!(
                    "{what} {}/{}",
                    human(l.up),
                    human(l.down)
                ))];
                if let Some(b) = &l.blocked {
                    spans.push(Span::styled(format!(" bloqueado: {b}"), novo));
                } else if l.new {
                    spans.push(Span::styled(" novo", novo));
                }
                if let Some(s) = &l.secret {
                    if l.proto == "ssh-sign" {
                        spans.push(Span::raw(format!(" assinatura ssh: {s}")));
                    } else {
                        spans.push(Span::raw(format!(" chave: {s}")));
                    }
                }
                lines.push(Line::from(spans));
            }
            // MCP por último: a rede, que muda a cada segundo, vem antes.
            lines.push(Line::raw(""));
            lines.push(Line::styled("MCP", head));
            if app.panel_mcp.is_empty() {
                lines.push(Line::raw("nenhum servidor"));
            }
            for m in &app.panel_mcp {
                lines.push(Line::raw(format!(
                    "{} {}",
                    if m.enabled { "✓" } else { "✗" },
                    m.name
                )));
            }
        }
        _ => lines.push(Line::raw(short("carregando…".into()))),
    }
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default().borders(Borders::LEFT).border_style(themed(
                theme,
                "modal-border",
                "",
                Style::default(),
            )),
        ),
        r,
    );
}

fn centered(full: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(full.width);
    let h = h.min(full.height);
    Rect::new(
        full.x + (full.width - w) / 2,
        full.y + (full.height - h) / 2,
        w,
        h,
    )
}

fn draw_modal(f: &mut Frame, full: Rect, m: &Modal, app: &App) {
    let theme = &app.cfg.theme;
    let selected = themed(
        theme,
        "selected-fg",
        "selected-bg",
        Style::default().add_modifier(Modifier::REVERSED),
    );
    let error = themed(theme, "error-fg", "", Style::default().fg(Color::Red));
    // Botão do modal de comando: linha e coluna dentro do quadro, e largura.
    let mut button: Option<(u16, u16, u16)> = None;
    let (title, lines, h): (&str, Vec<Line>, u16) = match m {
        Modal::Command(c) => {
            use super::CmdStage;
            let field = |sel: bool, label: &str, value: String| {
                let st = if sel { selected } else { Style::default() };
                Line::from(vec![
                    Span::raw(format!("{label:<10}")),
                    Span::styled(value, st),
                ])
            };
            let mut l = Vec::new();
            match c.stage {
                CmdStage::Ask => {
                    l.push(field(c.field == 0, "pergunta", format!("{} ", c.question)));
                    l.push(field(c.field == 1, "modelo", format!("{} ", c.model)));
                    l.push(Line::raw(""));
                    l.push(Line::raw(
                        "Tab troca de campo · ↑ ↓ modelos · Enter pergunta · Esc cancela",
                    ));
                }
                CmdStage::Waiting => {
                    l.push(Line::raw(format!("pergunta  {}", c.question)));
                    l.push(Line::raw(format!("modelo    {}", c.model)));
                    l.push(Line::raw(""));
                    l.push(Line::raw("esperando o modelo… (Esc cancela)"));
                }
                CmdStage::Command => {
                    button = Some((0, 0, 11));
                    l.push(Line::from(vec![
                        Span::styled("[ Inserir ]", selected),
                        Span::raw("  escreve na aba, sem executar"),
                    ]));
                    l.push(Line::raw(""));
                    for (i, line) in c.command.split('\n').enumerate() {
                        let label = if i == 0 { "comando" } else { "" };
                        l.push(Line::from(vec![
                            Span::raw(format!("{label:<10}")),
                            Span::styled(
                                line.to_string(),
                                Style::default().add_modifier(Modifier::BOLD),
                            ),
                        ]));
                    }
                    l.push(Line::raw(""));
                    l.push(Line::raw(
                        "Enter insere · edite antes, se quiser · Tab refaz a pergunta · Esc cancela",
                    ));
                }
            }
            if let Some(e) = &c.error {
                l.push(Line::styled(e.clone(), error));
            }
            let h = l.len() as u16 + 2;
            ("Comando", l, h)
        }
        Modal::New(n) => {
            let field = |i: usize, label: &str, value: String| {
                let st = if n.field == i {
                    selected
                } else {
                    Style::default()
                };
                Line::from(vec![
                    Span::raw(format!("{label:<10}")),
                    Span::styled(value, st),
                ])
            };
            let mut l = vec![
                field(0, "harness", format!("‹ {} ›", n.harness())),
                field(1, "pasta", format!("{} ", n.workdir)),
                field(2, "perfil", format!("{} ", n.profile)),
                Line::raw(""),
            ];
            if let Some(q) = &n.question {
                l.push(Line::raw(q.clone()));
            } else {
                l.push(Line::raw(
                    "Tab troca de campo · ← → harness · Enter cria · Esc cancela",
                ));
            }
            if let Some(e) = &n.error {
                l.push(Line::styled(e.clone(), error));
            }
            ("Nova aba", l, 10)
        }
        Modal::Chat(c) => {
            let field = |i: usize, label: &str, value: String| {
                let st = if c.field == i {
                    selected
                } else {
                    Style::default()
                };
                Line::from(vec![
                    Span::raw(format!("{label:<10}")),
                    Span::styled(value, st),
                ])
            };
            let mut l = vec![
                field(0, "mensagem", format!("{} ", c.text)),
                field(1, "modelo", format!("{} ", c.model)),
                Line::raw(""),
                Line::raw(
                    "Tab troca de campo · ↑ ↓ modelos da configuração · Enter envia · Esc cancela",
                ),
            ];
            if let Some(e) = &c.error {
                l.push(Line::styled(e.clone(), error));
            }
            ("Chat", l, 8)
        }
        Modal::List(sel) => {
            let l: Vec<Line> = app
                .tabs
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let st = if i == *sel {
                        selected
                    } else {
                        Style::default()
                    };
                    Line::styled(
                        format!(" {} {}  {}  {}", t.id, t.name, t.state, t.workdir.display()),
                        st,
                    )
                })
                .collect();
            let h = l.len() as u16 + 2;
            ("Abas (↑ ↓ Enter, Esc fecha)", l, h)
        }
        Modal::Rename(text) => (
            "Renomear aba (Enter grava, Esc cancela)",
            vec![Line::raw(format!("{text} "))],
            3,
        ),
        Modal::Close => (
            "Fechar aba",
            vec![Line::raw("Encerrar a sessão desta aba? s/n")],
            3,
        ),
        Modal::Mcp(ms) => {
            let mut l = vec![Line::raw(format!(
                "Servidores MCP da sessão {} (valem no próximo lançamento)",
                ms.id
            ))];
            if ms.list.is_empty() {
                l.push(Line::raw("nenhum servidor"));
            }
            for (i, m) in ms.list.iter().enumerate() {
                let row = format!(
                    "{} {:<16} {:<6} {:<8} {}",
                    if m.enabled { "✓" } else { "✗" },
                    m.name,
                    m.kind,
                    m.origin,
                    m.target
                );
                l.push(if i == ms.sel {
                    Line::styled(row, selected)
                } else {
                    Line::raw(row)
                });
            }
            l.push(Line::raw(""));
            l.push(Line::raw("espaço habilita/desabilita · ↑ ↓ · Esc fecha"));
            if let Some(n) = &ms.note {
                l.push(Line::raw(n.clone()));
            }
            let h = l.len() as u16 + 2;
            ("MCP", l, h)
        }
        Modal::Folders(fs) => {
            let mut l = vec![Line::raw(format!(
                "Pastas da sessão {} em /mnt/sandmux (valem na hora)",
                fs.id
            ))];
            if fs.list.is_empty() {
                l.push(Line::raw("nenhuma pasta"));
            }
            for (i, f) in fs.list.iter().enumerate() {
                let row = format!(
                    "{:<20} {}  {}",
                    f.dest().display(),
                    if f.rw { "rw" } else { "ro" },
                    f.path.display()
                );
                l.push(if i == fs.sel {
                    Line::styled(row, selected)
                } else {
                    Line::raw(row)
                });
            }
            l.push(Line::raw(""));
            l.push(match (&fs.input, &fs.ask) {
                (Some(t), _) => Line::styled(
                    format!("pasta: {t}▏ (Enter confirma, Esc cancela)"),
                    selected,
                ),
                (_, Some(p)) => Line::styled(
                    format!("{p}: r só leitura · w leitura e escrita · Esc cancela"),
                    selected,
                ),
                _ => Line::raw("a monta uma pasta · x desmonta · ↑ ↓ · Esc fecha"),
            });
            if let Some(n) = &fs.note {
                l.push(Line::raw(n.clone()));
            }
            let h = l.len() as u16 + 2;
            ("Pastas", l, h)
        }
        Modal::Files(fs) => {
            let mut l = vec![Line::raw(format!(
                "Acessos a arquivos da sessão {} (• nunca visto nesta pasta)",
                fs.id
            ))];
            let room = full.height.saturating_sub(10).max(3) as usize;
            let start = fs.sel.saturating_sub(room.saturating_sub(1));
            if fs.lines.is_empty() {
                l.push(Line::raw("sem acessos registrados desde ontem"));
            }
            for (i, x) in fs.lines.iter().enumerate().skip(start).take(room) {
                let hora = x.ts.split('T').nth(1).unwrap_or(&x.ts);
                let nota = x
                    .blocked
                    .as_deref()
                    .map(|b| format!(" bloqueado: {b}"))
                    .unwrap_or_default();
                let row = format!(
                    "{} {hora} {:<6} {}{nota}",
                    if x.new { "•" } else { " " },
                    x.access,
                    x.path.display()
                );
                l.push(if i == fs.sel {
                    Line::styled(row, selected)
                } else {
                    Line::raw(row)
                });
            }
            l.push(Line::raw(""));
            l.push(Line::raw("b bloqueia · l libera · ↑ ↓ · Esc fecha"));
            if let Some(n) = &fs.note {
                l.push(Line::raw(n.clone()));
            }
            let h = l.len() as u16 + 2;
            ("Arquivos", l, h)
        }
        Modal::Net(ns) => {
            let mut l = vec![Line::raw(format!(
                "Modo: {}; {}   (m troca o modo da aba)",
                ns.mode.describe(),
                ns.mode_origin
            ))];
            let tab = |i: usize, label: &str| {
                if ns.list == i {
                    Span::styled(format!(" {label} "), selected)
                } else {
                    Span::raw(format!(" {label} "))
                }
            };
            l.push(Line::from(vec![
                tab(0, "Acessos"),
                Span::raw(" "),
                tab(1, "Regras"),
                Span::raw("   (Tab alterna)"),
            ]));
            let room = full.height.saturating_sub(10).max(3) as usize;
            let sel = ns.sel[ns.list];
            let start = sel.saturating_sub(room.saturating_sub(1));
            let rows: Vec<String> = if ns.list == 0 {
                ns.lines
                    .iter()
                    .map(|x| {
                        let hora = x.ts.split('T').nth(1).unwrap_or(&x.ts);
                        let nota = match (&x.blocked, x.new) {
                            (Some(b), _) => format!("bloqueado: {b}"),
                            (None, true) => "novo".into(),
                            _ => String::new(),
                        };
                        format!(
                            "{hora} {:<4} {} {}:{} {}/{} {nota}",
                            x.proto,
                            x.domain,
                            x.ip,
                            x.port,
                            human(x.up),
                            human(x.down)
                        )
                    })
                    .collect()
            } else {
                ns.rules
                    .iter()
                    .map(|r| {
                        format!(
                            "{:<9} {:<5} {:<34} {}",
                            if r.enabled { "ligado" } else { "desligado" },
                            match r.kind {
                                sandmux_spec::net::RuleKind::Deny => "nega",
                                sandmux_spec::net::RuleKind::Allow => "libera",
                            },
                            r.pattern.to_string(),
                            r.origin
                        )
                    })
                    .collect()
            };
            if rows.is_empty() {
                l.push(Line::raw(if ns.list == 0 {
                    "sem acessos registrados desde ontem"
                } else {
                    "sem regras"
                }));
            }
            for (i, r) in rows.iter().enumerate().skip(start).take(room) {
                l.push(if i == sel {
                    Line::styled(r.clone(), selected)
                } else {
                    Line::raw(r.clone())
                });
            }
            l.push(Line::raw(""));
            l.push(match &ns.ask {
                Some(p) => Line::styled(p.prompt(), selected),
                None if ns.list == 0 => Line::raw("n nega · l libera · ↑ ↓ · Esc fecha"),
                None => Line::raw("espaço liga/desliga · x tira a regra · ↑ ↓ · Esc fecha"),
            });
            if let Some(n) = &ns.note {
                l.push(Line::styled(n.clone(), error));
            }
            let h = l.len() as u16 + 2;
            ("Rede da aba", l, h)
        }
        Modal::Unlock(text, err) => {
            let mut l = vec![
                Line::raw("O cofre de segredos está trancado."),
                Line::raw(format!("Senha: {}", "•".repeat(text.chars().count()))),
                Line::raw(""),
                Line::raw("Enter desbloqueia · Esc deixa trancado"),
            ];
            if let Some(e) = err {
                l.push(Line::styled(e.clone(), error));
            }
            let h = l.len() as u16 + 2;
            ("Cofre", l, h)
        }
        Modal::Help => {
            let p = app.prefix_name();
            let mut l = vec![Line::raw(format!("Tecla de prefixo: {p}"))];
            for (action, label) in super::HELP {
                l.push(Line::raw(format!(
                    "  {p} {}   {label}",
                    app.cfg.key(action)
                )));
            }
            l.push(Line::raw(format!("  {p} 1..9  ir para a aba")));
            l.push(Line::raw(format!("  {p} {p}  envia o prefixo ao harness")));
            let h = l.len() as u16 + 2;
            ("Ajuda (Esc fecha)", l, h)
        }
    };
    let width = lines
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(20)
        .max(title.chars().count() as u16 + 4)
        + 4;
    let r = centered(full, width.max(50), h.max(3));
    app.modal_button
        .set(button.map(|(li, col, w)| (r.x + 1 + col, r.y + 1 + li, w)));
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(themed(theme, "modal-border", "", Style::default()))
                .title_style(themed(theme, "modal-title", "", Style::default())),
        ),
        r,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cores_e_atributos() {
        let mut p = vt100::Parser::new(
            std::num::NonZeroU16::new(2).unwrap(),
            std::num::NonZeroU16::new(10).unwrap(),
            0,
        );
        p.process(b"\x1b[1;3;4;7;38;2;10;200;30;48;5;17mX\x1b[0m\x1b[31mY");
        let x = p.screen().cell(0, 0).unwrap();
        let s = cell_style(x);
        assert_eq!(s.fg, Some(Color::Rgb(10, 200, 30)));
        assert_eq!(s.bg, Some(Color::Indexed(17)));
        assert!(s.add_modifier.contains(
            Modifier::BOLD | Modifier::ITALIC | Modifier::UNDERLINED | Modifier::REVERSED
        ));
        let y = p.screen().cell(0, 1).unwrap();
        assert_eq!(cell_style(y).fg, Some(Color::Indexed(1)));
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
        draw_screen(
            &mut buf,
            Rect::new(0, 0, 10, 2),
            p.screen(),
            None,
            Style::default(),
        );
        assert_eq!(buf[(0, 0)].symbol(), "X");
        assert_eq!(buf[(1, 0)].symbol(), "Y");
    }

    #[test]
    fn estilo_do_tema() {
        let padrao = Theme::builtin("padrao").unwrap();
        let rev = Style::default().add_modifier(Modifier::REVERSED);
        assert_eq!(themed(&padrao, "tab-active-fg", "tab-active-bg", rev), rev);
        let mut t = Theme::builtin("escuro").unwrap();
        t.colors
            .insert("tab-active-bg".into(), ThemeColor::Rgb(0xaa, 0, 0));
        let s = themed(&t, "tab-active-fg", "tab-active-bg", rev);
        assert_eq!(s.bg, Some(Color::Rgb(0xaa, 0, 0)));
        assert!(!s.add_modifier.contains(Modifier::REVERSED));
    }
}
