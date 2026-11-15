//! Desenho: barra de abas, tela da aba ativa e modais.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Wrap};

use super::{App, Modal, Sel};
use sandmux_policy::config::{Theme, ThemeColor};
use sandmux_policy::limits::human;
use sandmux_proto::{Msg, SessionState};
use sandmux_spec::{tr, trf};

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
    format!(" {}:{}{} ", i + 1, t.name, tab_marks(t))
}

/// A barra sem símbolos especiais (`style = "simples"`).
fn draw_simple_bar(f: &mut Frame, bar: Rect, app: &mut App) {
    let theme = app.cfg.theme.clone();
    let mut spans = Vec::new();
    app.menu_button = None;
    app.bar_ranges.clear();
    app.close_ranges.clear();
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
        // O ✕ de uma aba encerrada fecha a aba: a coluna dele e a seguinte.
        if matches!(t.state, SessionState::Encerrada { .. }) {
            let at = x + format!(" {}:{} ", i + 1, t.name).chars().count() as u16;
            app.close_ranges.push((at, at + 2, i));
        }
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
}

/// Separadores inclinados do Powerline estendido.
const SEP_IN: &str = "\u{e0ba}";
const SEP_OUT: &str = "\u{e0bc}";

/// Cor de um elemento do tema, ou a do estilo Powerline padrão.
pub(super) fn pl(theme: &sandmux_policy::config::Theme, el: &str, padrao: &str) -> Color {
    theme
        .get(el)
        .or_else(|| sandmux_policy::config::ThemeColor::parse(padrao))
        .map(tcolor)
        .unwrap_or(Color::Reset)
}

/// Duração curta: `1d 3h`, `2h 15m` ou `7m`.
pub fn short_duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

/// Uma pasta com `~` no lugar do HOME.
fn tilde(p: &std::path::Path, home: &std::path::Path) -> String {
    match p.strip_prefix(home) {
        Ok(r) if r.as_os_str().is_empty() => "~".into(),
        Ok(r) => format!("~/{}", r.display()),
        Err(_) => p.display().to_string(),
    }
}

/// Os blocos da direita, na ordem em que aparecem: o harness e a pasta da
/// aba ativa (em blocos separados), o cofre, os alertas de rede e o tempo. Os ícones só com
/// `icons` (precisam de uma Nerd Font).
fn chips(app: &App, theme: &Theme, icons: bool) -> Vec<(String, Color)> {
    let ic = |i: &str, s: String| if icons { format!("{i} {s}") } else { s };
    let mut chips: Vec<(String, Color)> = Vec::new();
    if let Some(t) = app.tabs.get(app.active) {
        chips.push((
            ic("\u{f120}", t.opts.harness.clone()),
            pl(theme, "chip-harness-bg", "#f38ba8"),
        ));
        chips.push((
            ic("\u{f07b}", tilde(&t.workdir, &app.host.home)),
            pl(theme, "chip-folder-bg", "#cba6f7"),
        ));
    }
    if let Some((exists, unlocked)) = app.vault {
        let (i, text) = match (exists, unlocked) {
            (false, _) => ("\u{f09c}", tr!("sem cofre", "no vault")),
            (true, true) => ("\u{f09c}", tr!("cofre aberto", "vault open")),
            (true, false) => ("\u{f023}", tr!("cofre trancado", "vault locked")),
        };
        chips.push((ic(i, text.into()), pl(theme, "chip-vault-bg", "#a6e3a1")));
    }
    let blocked = app.tabs.iter().filter(|t| t.blocked).count();
    let new = app.tabs.iter().filter(|t| t.new_domain).count();
    let keys = app.tabs.iter().filter(|t| t.keys.any()).count();
    if blocked + new + keys > 0 {
        let mut parts = Vec::new();
        if blocked > 0 {
            parts.push(format!("! {blocked}"));
        }
        if new > 0 {
            parts.push(format!("• {new}"));
        }
        if keys > 0 {
            parts.push(format!("✱ {keys}"));
        }
        chips.push((
            ic("\u{f071}", parts.join("  ")),
            pl(theme, "chip-alert-bg", "#f9e2af"),
        ));
    }
    if let Some(t) = app.tabs.get(app.active)
        && t.state == SessionState::Viva
        && let Some(st) = t.started_at
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(st);
        chips.push((
            ic("\u{f017}", short_duration(now.saturating_sub(st))),
            pl(theme, "chip-time-bg", "#89dceb"),
        ));
    }
    chips
}

/// Marca de estado e de rede de uma aba, depois do nome.
fn tab_marks(t: &super::Tab) -> String {
    let mark = match &t.state {
        SessionState::Viva => "",
        SessionState::Encerrada { .. } => " ✕",
        SessionState::Parada { .. } => " ‖",
        SessionState::AguardandoCofre => tr!(" (cofre)", " (vault)"),
        SessionState::Reiniciando { .. } => " ↻",
    };
    let net = match (t.blocked, t.new_domain) {
        (true, true) => " !•",
        (true, false) => " !",
        (false, true) => " •",
        _ => "",
    };
    let keys = if t.keys.any() { " ✱" } else { "" };
    format!("{mark}{net}{keys}")
}

/// O texto das chaves reais de uma aba, para o painel.
fn keys_text(k: &sandmux_proto::msg::KeyNote) -> String {
    if k.any() {
        k.to_string()
    } else {
        tr!("nenhuma", "none").into()
    }
}

/// A barra em blocos (o padrão): o `≡` do menu, as abas com o número num
/// bloco, os avisos no meio e os blocos da direita. Só usa caracteres
/// comuns, que toda fonte tem.
fn draw_blocks_bar(f: &mut Frame, bar: Rect, app: &mut App) {
    let theme = app.cfg.theme.clone();
    let bg = pl(&theme, "bar-bg", "#1e1e2e");
    let surface = pl(&theme, "tab-bg", "#313244");
    let idx_bg = pl(&theme, "tab-index-bg", "#45475a");
    let idx_fg = pl(&theme, "tab-index-fg", "#cdd6f4");
    let act_bg = pl(&theme, "tab-active-index-bg", "#cba6f7");
    let act_fg = pl(&theme, "tab-active-index-fg", "#1e1e2e");
    let name_fg = pl(&theme, "tab-fg", "#a6adc8");
    let chip_fg = pl(&theme, "chip-fg", "#1e1e2e");
    let accent = pl(&theme, "accent", "#cba6f7");
    let base = Style::default().bg(bg);
    let mut spans: Vec<Span> = Vec::new();
    app.bar_ranges.clear();
    app.close_ranges.clear();
    spans.push(Span::styled(
        " ≡ ",
        Style::default()
            .fg(chip_fg)
            .bg(accent)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(" ", base));
    app.menu_button = Some((0, 3));
    let mut x = 4u16;
    for (i, t) in app.tabs.iter().enumerate() {
        let active = i == app.active;
        let num = format!(" {} ", i + 1);
        let name = format!(" {}{} ", t.name, tab_marks(t));
        let nw = num.chars().count() as u16;
        let w = nw + name.chars().count() as u16;
        app.bar_ranges.push((x, x + w, i));
        if matches!(t.state, SessionState::Encerrada { .. }) {
            let at = x + nw + 1 + t.name.chars().count() as u16 + 1;
            app.close_ranges.push((at, at + 2, i));
        }
        x += w + 1;
        let (nbg, nfg) = if active {
            (act_bg, act_fg)
        } else {
            (idx_bg, idx_fg)
        };
        spans.push(Span::styled(
            num,
            Style::default()
                .fg(nfg)
                .bg(nbg)
                .add_modifier(Modifier::BOLD),
        ));
        let ns = if active {
            Style::default()
                .fg(act_bg)
                .bg(surface)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(name_fg).bg(surface)
        };
        spans.push(Span::styled(name, ns));
        spans.push(Span::styled(" ", base));
    }
    let mut chips = chips(app, &theme, false);
    let chip_w = |c: &(String, Color)| c.0.chars().count() as u16 + 3;
    while !chips.is_empty() && x + chips.iter().map(chip_w).sum::<u16>() > bar.width {
        chips.remove(0);
    }
    let right: u16 = chips.iter().map(chip_w).sum::<u16>().saturating_sub(1);
    let free = bar.width.saturating_sub(x + right);
    let status = app.status_text();
    if !status.is_empty() && free > 3 {
        let s: String = status.chars().take(free as usize - 2).collect();
        let pad = free as usize - s.chars().count();
        spans.push(Span::styled(
            s,
            themed(&theme, "status-fg", "", Style::default().fg(Color::Yellow))
                .bg(bg)
                .add_modifier(Modifier::ITALIC),
        ));
        spans.push(Span::styled(" ".repeat(pad), base));
    } else {
        spans.push(Span::styled(" ".repeat(free as usize), base));
    }
    let last = chips.len();
    for (n, (text, cbg)) in chips.into_iter().enumerate() {
        spans.push(Span::styled(
            format!(" {text} "),
            Style::default()
                .fg(chip_fg)
                .bg(cbg)
                .add_modifier(Modifier::BOLD),
        ));
        if n + 1 < last {
            spans.push(Span::styled(" ", base));
        }
    }
    f.render_widget(Paragraph::new(Line::from(spans)).style(base), bar);
}

/// A barra no estilo Powerline: abas em blocos, avisos no meio e, à
/// direita, o harness, o cofre, os alertas de rede e o tempo da aba ativa.
fn draw_powerline_bar(f: &mut Frame, bar: Rect, app: &mut App) {
    let theme = app.cfg.theme.clone();
    let bg = pl(&theme, "bar-bg", "#1e1e2e");
    let idx_bg = pl(&theme, "tab-index-bg", "#45475a");
    let idx_fg = pl(&theme, "tab-index-fg", "#cdd6f4");
    let act_bg = pl(&theme, "tab-active-index-bg", "#cba6f7");
    let act_fg = pl(&theme, "tab-active-index-fg", "#1e1e2e");
    let name_fg = pl(&theme, "tab-fg", "#a6adc8");
    let chip_fg = pl(&theme, "chip-fg", "#1e1e2e");
    let base = Style::default().bg(bg);
    let mut spans: Vec<Span> = Vec::new();
    app.bar_ranges.clear();
    app.close_ranges.clear();
    let accent = pl(&theme, "accent", "#cba6f7");
    spans.push(Span::styled(
        " ≡ ",
        Style::default()
            .fg(chip_fg)
            .bg(accent)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(SEP_OUT, Style::default().fg(accent).bg(bg)));
    app.menu_button = Some((0, 3));
    let mut x = 4u16;
    for (i, t) in app.tabs.iter().enumerate() {
        let active = i == app.active;
        let (cbg, cfg) = if active {
            (act_bg, act_fg)
        } else {
            (idx_bg, idx_fg)
        };
        let num = format!(" {} ", i + 1);
        let name = format!(" {}{} ", t.name, tab_marks(t));
        let w = 1 + num.chars().count() as u16 + 1 + name.chars().count() as u16;
        app.bar_ranges.push((x, x + w, i));
        if matches!(t.state, SessionState::Encerrada { .. }) {
            let at = x + 1 + num.chars().count() as u16 + 1 + 1 + t.name.chars().count() as u16 + 1;
            app.close_ranges.push((at, at + 2, i));
        }
        x += w;
        spans.push(Span::styled(SEP_IN, Style::default().fg(cbg).bg(bg)));
        spans.push(Span::styled(
            num,
            Style::default()
                .fg(cfg)
                .bg(cbg)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(SEP_OUT, Style::default().fg(cbg).bg(bg)));
        let ns = if active {
            Style::default()
                .fg(act_bg)
                .bg(bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(name_fg).bg(bg)
        };
        spans.push(Span::styled(name, ns));
    }

    let mut chips = chips(app, &theme, true);
    let chip_w = |c: &(String, Color)| 1 + c.0.chars().count() as u16 + 2;
    // Sem espaço, os blocos saem da esquerda; o tempo fica por último.
    while !chips.is_empty() && x + chips.iter().map(chip_w).sum::<u16>() + 1 > bar.width {
        chips.remove(0);
    }
    let right: u16 = chips.iter().map(chip_w).sum();
    let free = bar.width.saturating_sub(x + right);
    let status = app.status_text();
    if !status.is_empty() && free > 4 {
        let s: String = status.chars().take(free as usize - 2).collect();
        let pad = free as usize - s.chars().count();
        spans.push(Span::styled(" ", base));
        spans.push(Span::styled(
            s,
            themed(&theme, "status-fg", "", Style::default().fg(Color::Yellow))
                .bg(bg)
                .add_modifier(Modifier::ITALIC),
        ));
        spans.push(Span::styled(" ".repeat(pad.saturating_sub(1)), base));
    } else {
        spans.push(Span::styled(" ".repeat(free as usize), base));
    }
    let mut prev = bg;
    for (text, cbg) in chips {
        spans.push(Span::styled(SEP_IN, Style::default().fg(cbg).bg(prev)));
        spans.push(Span::styled(
            format!(" {text} "),
            Style::default()
                .fg(chip_fg)
                .bg(cbg)
                .add_modifier(Modifier::BOLD),
        ));
        prev = cbg;
    }
    f.render_widget(Paragraph::new(Line::from(spans)).style(base), bar);
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let full = f.area();
    // A barra fica na última linha (padrão) ou na primeira (`bar = "top"`).
    let top = app.cfg.bar_top;
    let bar = if top {
        Rect::new(full.x, full.y, full.width, 1)
    } else {
        Rect::new(
            full.x,
            full.y + full.height.saturating_sub(1),
            full.width,
            1,
        )
    };
    let area_y = if top { full.y + 1 } else { full.y };
    app.bar_row = bar.y + 1;
    app.area_dy = if top { 1 } else { 0 };
    let panel_on = super::panel_visible(app.panel, full.width);
    let panel_w = if panel_on { super::PANEL } else { 0 };
    let area = Rect::new(
        full.x,
        area_y,
        full.width - panel_w,
        full.height.saturating_sub(1),
    );
    if panel_on {
        let r = Rect::new(
            full.x + full.width - panel_w,
            area_y,
            panel_w,
            full.height.saturating_sub(1),
        );
        draw_panel(f, r, app);
    }

    // Barra de abas.
    let theme = app.cfg.theme.clone();
    match app.cfg.style {
        sandmux_policy::config::BarStyle::Blocos => draw_blocks_bar(f, bar, app),
        sandmux_policy::config::BarStyle::Powerline => draw_powerline_bar(f, bar, app),
        sandmux_policy::config::BarStyle::Simples => draw_simple_bar(f, bar, app),
    }

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
            SessionState::Encerrada { codigo } => Some(trf!(
                " encerrada ({codigo}) · {} r reinicia · {} x fecha ",
                " ended ({codigo}) · {} r restarts · {} x closes ",
                app.prefix_name(),
                app.prefix_name()
            )),
            SessionState::Parada { motivo } => Some(trf!(
                " parada: {motivo} · {} r reinicia · {} x fecha ",
                " stopped: {motivo} · {} r restarts · {} x closes ",
                app.prefix_name(),
                app.prefix_name()
            )),
            SessionState::Viva if t.scroll > 0 => {
                Some(trf!(
                    " histórico: {} linhas acima ",
                    " history: {} lines up ",
                    t.scroll
                ))
            }
            SessionState::Viva => None,
            SessionState::AguardandoCofre => Some(
                tr!(
                    " aguardando cofre: a sessão sobe quando o cofre for desbloqueado (sandmux vault unlock) ",
                    " waiting for vault: the session starts once the vault is unlocked (sandmux vault unlock) "
                )
                .to_string(),
            ),
            SessionState::Reiniciando { segundos } => Some(trf!(
                " o gateway terminou e volta em {segundos} s · {} x fecha ",
                " the gateway ended and comes back in {segundos} s · {} x closes ",
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

    match &app.modal {
        Some(Modal::Settings(s)) => draw_settings(f, full, s, app),
        Some(m) => draw_modal(f, full, m, app),
        None => {}
    }
    if app.picker.is_some() {
        super::picker::draw(f, full, app);
    }
}

/// A tecla de uma ação na ajuda e no menu; sem tecla, um travessão.
fn key_label(c: char) -> String {
    if c == '\0' {
        "—".into()
    } else {
        c.to_string()
    }
}

/// Escreve uma linha numa posição, cortada na largura.
pub(super) fn put(f: &mut Frame, x: u16, y: u16, w: u16, line: Line, style: Style) {
    let area = f.area();
    if y >= area.y + area.height || x >= area.x + area.width || w == 0 {
        return;
    }
    let w = w.min(area.x + area.width - x);
    f.render_widget(Paragraph::new(line).style(style), Rect::new(x, y, w, 1));
}

/// Texto cortado em `n` caracteres, com `…`.
pub(super) fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut o: String = s.chars().take(n.saturating_sub(1)).collect();
        o.push('…');
        o
    }
}

/// A página de configurações, em tela cheia: seções à esquerda, itens à
/// direita, a ajuda do item e o último aviso embaixo.
fn draw_settings(f: &mut Frame, full: Rect, s: &super::settings::Settings, app: &App) {
    use super::Hit;
    use super::settings::{At, Kind, SECTIONS, Source, current, items_of, section_name};
    // O tema `padrao` (as cores do terminal) desenha a página com o escuro.
    let theme = if app.cfg.theme.colors.is_empty() {
        Theme::builtin("escuro").unwrap_or_default()
    } else {
        app.cfg.theme.clone()
    };
    let c = |el: &str| pl(&theme, el, "default");
    let (bg, fg, surface, accent) = (c("page-bg"), c("page-fg"), c("surface-bg"), c("accent"));
    let (muted, ok, err, on_accent) = (c("muted-fg"), c("ok-fg"), c("error-fg"), c("selected-fg"));
    let base = Style::default().fg(fg).bg(bg);
    let mut hits = app.settings_hits.borrow_mut();
    hits.clear();

    f.render_widget(Clear, full);
    let target = match s.project() {
        Some(p) => trf!(
            " valendo em: projeto {} ",
            " applies to: project {} ",
            tilde(p, &app.host.home)
        ),
        None => tr!(" valendo em: global ", " applies to: global ").to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent).bg(bg))
        .style(base)
        .title(Line::styled(
            tr!(" sandmux · configurações ", " sandmux · settings "),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
        .title_top(
            Line::styled(target, Style::default().fg(muted)).right_aligned(),
        )
        .title_bottom(Line::styled(
            tr!(
                " ↑ ↓ move · Tab seções/itens · ← → muda · Espaço liga · Enter edita · x padrão · g alvo · Esc sai ",
                " ↑ ↓ move · Tab sections/items · ← → change · Space toggles · Enter edits · x default · g target · Esc quits "
            ),
            Style::default().fg(muted),
        ));
    let inner = block.inner(full);
    f.render_widget(block, full);
    if inner.width < 30 || inner.height < 6 {
        return;
    }

    // Seções.
    let side = 22u16.min(inner.width / 3);
    let side_r = Rect::new(inner.x, inner.y, side, inner.height);
    f.render_widget(Block::default().style(Style::default().bg(surface)), side_r);
    for i in 0..SECTIONS.len() {
        let name = section_name(i);
        let y = inner.y + 1 + i as u16;
        if y >= inner.y + inner.height {
            break;
        }
        let sel = i == s.section;
        let (style, mark) = match (sel, s.on_items) {
            (true, false) => (
                Style::default()
                    .fg(on_accent)
                    .bg(accent)
                    .add_modifier(Modifier::BOLD),
                " ",
            ),
            (true, true) => (
                Style::default()
                    .fg(accent)
                    .bg(surface)
                    .add_modifier(Modifier::BOLD),
                "▌",
            ),
            _ => (Style::default().fg(muted).bg(surface), " "),
        };
        let text = format!("{mark} {name}");
        let pad = side as usize - text.chars().count().min(side as usize);
        put(
            f,
            side_r.x,
            y,
            side,
            Line::raw(format!("{text}{}", " ".repeat(pad))),
            style,
        );
        hits.push((y, side_r.x, side_r.x + side, Hit::Section(i)));
    }

    // Itens da seção.
    let x0 = inner.x + side + 2;
    let w = inner.width.saturating_sub(side + 3);
    let mut y = inner.y + 1;
    put(
        f,
        x0,
        y,
        w,
        Line::raw(section_name(s.section)),
        Style::default()
            .fg(accent)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    );
    y += 2;
    let ids = items_of(s.section);
    let label_w = 26usize;
    let help_rows = 4u16;
    let bottom = inner.y + inner.height;
    let selected_item = s.selected();

    if s.section == super::settings::VAULT {
        let v = &s.vault;
        let status = match v.info {
            None => Line::styled(tr!("lendo o cofre…", "reading the vault…"), base.fg(muted)),
            Some((false, _, _)) => Line::from(vec![
                Span::styled("○ ", base.fg(muted)),
                Span::styled(tr!("sem cofre", "no vault"), base),
            ]),
            Some((true, false, _)) => Line::from(vec![
                Span::styled("● ", base.fg(c("status-fg"))),
                Span::styled(tr!("trancado", "locked"), base),
            ]),
            Some((true, true, n)) => Line::from(vec![
                Span::styled("● ", base.fg(ok)),
                Span::styled(
                    if n == 1 {
                        tr!("aberto · 1 segredo", "open · 1 secret").to_string()
                    } else {
                        trf!("aberto · {n} segredos", "open · {n} secrets")
                    },
                    base,
                ),
            ]),
        };
        let mut status = status;
        match &v.keyring {
            Some((true, None)) => status.spans.push(Span::styled(
                tr!(" · keyring ligado", " · keyring on"),
                base.fg(muted),
            )),
            Some((true, Some(e))) => status.spans.push(Span::styled(
                trf!(" · keyring: {e}", " · keyring: {e}"),
                base.fg(c("status-fg")),
            )),
            _ => {}
        }
        put(f, x0, y, w, status, base);
        y += 2;
        let nw = 20usize;
        let kw = 6usize;
        if !v.entries.is_empty() {
            put(
                f,
                x0,
                y,
                w,
                Line::styled(
                    format!(
                        " {:<nw$} {:<kw$} {}",
                        tr!("nome", "name"),
                        tr!("tipo", "kind"),
                        tr!("domínios", "domains")
                    ),
                    base.fg(muted).add_modifier(Modifier::UNDERLINED),
                ),
                base,
            );
            y += 1;
        }
        let room = bottom.saturating_sub(y + help_rows + 4) as usize;
        let start = v.sel.saturating_sub(room.saturating_sub(1));
        for (i, e) in v.entries.iter().enumerate().skip(start).take(room.max(1)) {
            let sel = s.on_items && i == v.sel && v.form.is_none();
            let row_bg = if sel { surface } else { bg };
            let kind = if e.kind == "ssh" { "ssh" } else { "token" };
            let doms = e.domains.join(", ");
            let line = Line::from(vec![
                Span::styled(
                    if sel { "▌" } else { " " },
                    Style::default().fg(accent).bg(row_bg),
                ),
                Span::styled(
                    format!("{:<nw$} ", cut(&e.name, nw)),
                    Style::default().fg(fg).bg(row_bg).add_modifier(if sel {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
                Span::styled(
                    format!("{kind:<kw$} "),
                    Style::default().fg(muted).bg(row_bg),
                ),
                Span::styled(
                    cut(&doms, (w as usize).saturating_sub(nw + kw + 3)),
                    Style::default().fg(fg).bg(row_bg),
                ),
            ]);
            put(f, x0, y, w, line, Style::default().bg(row_bg));
            hits.push((y, x0, x0 + w, Hit::Item(i)));
            y += 1;
        }
        if v.entries.is_empty() && matches!(v.info, Some((true, true, _))) {
            put(
                f,
                x0,
                y,
                w,
                Line::styled(tr!(" nenhum segredo", " no secrets"), base.fg(muted)),
                base,
            );
            y += 1;
        }
        y += 1;
        if let Some(form) = &v.form {
            put(
                f,
                x0,
                y,
                w,
                Line::styled(
                    form.kind.title(),
                    base.fg(accent).add_modifier(Modifier::BOLD),
                ),
                base,
            );
            y += 1;
            for (i, ((labels, secret), text)) in form
                .kind
                .fields()
                .iter()
                .zip(form.fields.iter())
                .enumerate()
            {
                let label = tr!(labels[0], labels[1]);
                let shown = if *secret {
                    "•".repeat(text.chars().count())
                } else {
                    text.to_string()
                };
                let cur = i == form.step;
                let line = Line::from(vec![
                    Span::styled(format!(" {label:<30} "), base.fg(muted)),
                    Span::styled(
                        format!("{shown}{}", if cur { "▏" } else { "" }),
                        if cur {
                            Style::default()
                                .fg(fg)
                                .bg(surface)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            base
                        },
                    ),
                ]);
                put(f, x0, y, w, line, base);
                y += 1;
            }
            put(
                f,
                x0,
                y + 1,
                w,
                Line::styled(
                    tr!("Enter avança · Esc cancela", "Enter goes on · Esc cancels"),
                    base.fg(muted),
                ),
                base,
            );
        } else if let Some(name) = &v.confirm {
            put(
                f,
                x0,
                y,
                w,
                Line::styled(
                    trf!(
                        "Apagar {name} do cofre? s confirma · outra tecla cancela",
                        "Remove {name} from the vault? y confirms · any other key cancels"
                    ),
                    base.fg(err).add_modifier(Modifier::BOLD),
                ),
                base,
            );
        } else {
            let keys = match v.info {
                Some((false, _, _)) => tr!("n cria o cofre", "n creates the vault"),
                Some((true, false, _)) => {
                    tr!(
                        "u desbloqueia · p troca a senha · k keyring · r relê",
                        "u unlocks · p changes the password · k keyring · r reloads"
                    )
                }
                _ => tr!(
                    "a segredo · s chave SSH · d domínios · x apaga · l tranca · p senha · k keyring · r relê",
                    "a secret · s SSH key · d domains · x removes · l locks · p password · k keyring · r reloads"
                ),
            };
            put(f, x0, y, w, Line::styled(keys, base.fg(muted)), base);
        }
        let hy = bottom.saturating_sub(help_rows);
        put(
            f,
            x0,
            hy,
            w,
            Line::styled(
                tr!(
                    "Os valores nunca aparecem. Tab ou → entra na lista.",
                    "Values are never shown. Tab or → enters the list."
                ),
                base.fg(muted),
            ),
            base,
        );
        if let Some((note, is_err)) = &s.note {
            put(
                f,
                x0,
                hy + 2,
                w,
                Line::raw(note.clone()),
                Style::default()
                    .fg(if *is_err { err } else { ok })
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            );
        }
        return;
    }

    if let (Some(l), Some(item)) = (&s.list, selected_item) {
        // Editor de lista.
        let cur = current(&s.doc, item, s.project());
        let place = if item.at == At::Layer {
            match s.project() {
                Some(p) => trf!("projeto {}", "project {}", tilde(p, &app.host.home)),
                None => "global".into(),
            }
        } else {
            "global".into()
        };
        put(
            f,
            x0,
            y,
            w,
            Line::from(vec![
                Span::styled(
                    item.label(),
                    Style::default().fg(fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ({}, {place})", item.key),
                    Style::default().fg(muted),
                ),
            ]),
            base,
        );
        y += 2;
        if cur.list.is_empty() || cur.from != Source::Here {
            let t = if cur.from == Source::Global && !cur.list.is_empty() {
                trf!(
                    "nada neste projeto; o global tem {}",
                    "nothing in this project; global has {}",
                    cur.list.join(", ")
                )
            } else {
                tr!("vazia", "empty").into()
            };
            put(f, x0, y, w, Line::raw(t), Style::default().fg(muted).bg(bg));
            y += 1;
        }
        if cur.from == Source::Here {
            for (i, v) in cur.list.iter().enumerate() {
                if y + help_rows >= bottom {
                    break;
                }
                let st = if i == l.sel && l.input.is_none() {
                    Style::default()
                        .fg(on_accent)
                        .bg(accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(fg).bg(surface)
                };
                put(
                    f,
                    x0,
                    y,
                    w,
                    Line::raw(format!(" {} ", cut(v, w as usize - 2))),
                    st,
                );
                y += 1;
            }
        }
        y += 1;
        let line = match &l.input {
            Some(text) => Line::from(vec![
                Span::styled(tr!("novo: ", "new: "), Style::default().fg(muted)),
                Span::styled(
                    format!("{text}▏"),
                    Style::default()
                        .fg(fg)
                        .bg(surface)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    tr!(
                        "  Enter acrescenta · Esc cancela",
                        "  Enter adds · Esc cancels"
                    ),
                    Style::default().fg(muted),
                ),
            ]),
            None => Line::styled(
                tr!(
                    "a acrescenta · x tira o escolhido · ↑ ↓ · Esc volta",
                    "a adds · x removes the selected one · ↑ ↓ · Esc goes back"
                ),
                Style::default().fg(muted),
            ),
        };
        put(f, x0, y, w, line, base);
    } else {
        let room = bottom.saturating_sub(y + help_rows + 2) as usize;
        let start = s.item.saturating_sub(room.saturating_sub(1));
        for (pos, idx) in ids.iter().enumerate().skip(start).take(room.max(1)) {
            let item = &super::settings::ITEMS[*idx];
            let cur = current(&s.doc, item, s.project());
            let sel = s.on_items && pos == s.item;
            let row_bg = if sel { surface } else { bg };
            let mut spans = vec![
                Span::styled(
                    if sel { "▌ " } else { "  " },
                    Style::default().fg(accent).bg(row_bg),
                ),
                Span::styled(
                    format!("{:<label_w$}", item.label()),
                    Style::default().fg(fg).bg(row_bg).add_modifier(if sel {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
            ];
            let val = Style::default().bg(row_bg);
            match item.kind {
                Kind::Toggle => {
                    if cur.bool == Some(true) {
                        spans.push(Span::styled(
                            tr!("● ligado", "● on"),
                            val.fg(ok).add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        spans.push(Span::styled(tr!("○ desligado", "○ off"), val.fg(muted)));
                    }
                }
                Kind::Choice(_) => spans.push(Span::styled(
                    format!(
                        "‹ {} ›",
                        sandmux_policy::config::value_label(
                            item.key,
                            &cur.value.clone().unwrap_or_default()
                        )
                    ),
                    val.fg(accent).add_modifier(Modifier::BOLD),
                )),
                Kind::Text => match (&s.edit, sel) {
                    (Some(text), true) => spans.push(Span::styled(
                        format!(" {text}▏ "),
                        val.fg(fg).bg(bg).add_modifier(Modifier::BOLD),
                    )),
                    _ => spans.push(match &cur.value {
                        Some(v) => Span::styled(v.clone(), val.fg(fg)),
                        None => Span::styled(tr!("padrão", "default"), val.fg(muted)),
                    }),
                },
                Kind::List => {
                    let n = cur.list.len();
                    spans.push(if n == 0 {
                        Span::styled(tr!("vazia", "empty"), val.fg(muted))
                    } else {
                        Span::styled(
                            cut(
                                &cur.list.join(", "),
                                (w as usize).saturating_sub(label_w + 16),
                            ),
                            val.fg(fg),
                        )
                    });
                }
            }
            let origin = if s.on_project && item.at != At::Layer {
                tr!("  só global", "  global only")
            } else {
                match cur.from {
                    Source::Here => "",
                    Source::Global => tr!("  do global", "  from global"),
                    Source::Default if matches!(item.kind, Kind::Text | Kind::List) => "",
                    Source::Default => tr!("  padrão", "  default"),
                }
            };
            if !origin.is_empty() {
                spans.push(Span::styled(
                    origin,
                    val.fg(muted).add_modifier(Modifier::ITALIC),
                ));
            }
            put(f, x0, y, w, Line::from(spans), Style::default().bg(row_bg));
            hits.push((y, x0, x0 + w, Hit::Item(pos)));
            y += 1;
        }
        // Amostras do tema, na seção de aparência.
        if s.section == 0 && y + help_rows + 1 < bottom {
            y += 1;
            let mut spans = vec![Span::styled(
                format!("  {:<label_w$}", tr!("Amostra", "Sample")),
                Style::default().fg(muted).bg(bg),
            )];
            for el in [
                "accent",
                "chip-harness-bg",
                "chip-folder-bg",
                "chip-vault-bg",
                "chip-alert-bg",
                "chip-time-bg",
                "error-fg",
                "surface-bg",
                "muted-fg",
            ] {
                spans.push(Span::styled("███", Style::default().fg(c(el)).bg(bg)));
                spans.push(Span::styled(" ", Style::default().bg(bg)));
            }
            put(f, x0, y, w, Line::from(spans), base);
        }
    }

    // Ajuda do item e aviso.
    let hy = bottom.saturating_sub(help_rows);
    if let Some(item) = selected_item {
        let r = Rect::new(x0, hy, w, 2);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{}: ", item.key), Style::default().fg(accent)),
                Span::styled(item.help(), Style::default().fg(muted)),
            ]))
            .wrap(Wrap { trim: true })
            .style(base),
            r,
        );
    }
    if let Some((note, is_err)) = &s.note {
        put(
            f,
            x0,
            hy + 2,
            w,
            Line::raw(note.clone()),
            Style::default()
                .fg(if *is_err { err } else { ok })
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        );
    }
}

/// Medidor de uso: `█` até a fração usada, `░` no resto, verde, amarelo
/// acima de 75% e vermelho acima de 90%.
fn meter(
    used: u64,
    max: u64,
    w: usize,
    colors: (Color, Color, Color),
    rest: Style,
) -> Line<'static> {
    let frac = if max == 0 {
        0.0
    } else {
        (used as f64 / max as f64).clamp(0.0, 1.0)
    };
    let mut full = ((frac * w as f64).round() as usize).min(w);
    if used > 0 && full == 0 && w > 0 {
        full = 1;
    }
    let c = if frac > 0.9 {
        colors.2
    } else if frac > 0.75 {
        colors.1
    } else {
        colors.0
    };
    Line::from(vec![
        Span::styled("█".repeat(full), Style::default().fg(c)),
        Span::styled("░".repeat(w - full), rest),
    ])
}

/// Painel lateral: proteção, recursos, portas, rede e MCP da aba ativa, cada
/// seção com uma faixa de título.
fn draw_panel(f: &mut Frame, r: Rect, app: &App) {
    let theme = &app.cfg.theme;
    let accent = pl(theme, "accent", "#cba6f7");
    let surface = pl(theme, "surface-bg", "#313244");
    let ok = pl(theme, "ok-fg", "#a6e3a1");
    let warn = pl(theme, "status-fg", "#f9e2af");
    let bad = pl(theme, "error-fg", "#f38ba8");
    // Sem cor no tema, o texto secundário fica esmaecido.
    let muted = match theme.get("muted-fg") {
        Some(c) => Style::default().fg(tcolor(c)),
        None => Style::default().add_modifier(Modifier::DIM),
    };
    // Largura útil: tira a borda e a margem.
    let w = r.width.saturating_sub(2) as usize;
    let home = std::env::var("HOME").unwrap_or_default();
    // O HOME aparece como `~`; as linhas longas quebram.
    let short = |s: String| {
        if home.is_empty() {
            s
        } else {
            s.replace(&home, "~")
        }
    };
    // Cada seção: chave, título, resumo e linhas.
    let mut secs: Vec<(&'static str, &'static str, String, Vec<Line>)> = Vec::new();
    fn start(
        secs: &mut Vec<(&'static str, &'static str, String, Vec<Line<'static>>)>,
        key: &'static str,
        title: &'static str,
        right: String,
    ) {
        secs.push((key, title, right, Vec::new()));
    }
    fn push(
        secs: &mut [(&'static str, &'static str, String, Vec<Line<'static>>)],
        l: Line<'static>,
    ) {
        if let Some(s) = secs.last_mut() {
            s.3.push(l);
        }
    }
    // Rótulo à esquerda, apagado; valor alinhado à direita. Sem espaço, o
    // valor vai para a linha de baixo.
    let row = |label: &str, value: String, vstyle: Style| -> Vec<Line<'static>> {
        let (l, v) = (label.chars().count(), value.chars().count());
        if l + v + 2 <= w {
            vec![Line::from(vec![
                Span::styled(format!(" {label}"), muted),
                Span::raw(" ".repeat(w - l - v - 1)),
                Span::styled(value, vstyle),
            ])]
        } else {
            vec![
                Line::styled(format!(" {label}"), muted),
                Line::styled(format!("   {value}"), vstyle),
            ]
        }
    };
    // Grade de pontos de estado em duas colunas: ● ligado, ○ desligado.
    let dots = |items: &[(bool, String, Color)]| -> Vec<Line<'static>> {
        let col = w / 2;
        items
            .chunks(2)
            .map(|pair| {
                let mut spans = Vec::new();
                for (i, (on, label, c)) in pair.iter().enumerate() {
                    let text = cut(label, col.saturating_sub(4));
                    let cell = col.saturating_sub(if i == 0 { 1 } else { 0 });
                    let pad = cell.saturating_sub(text.chars().count() + 3);
                    if i == 0 {
                        spans.push(Span::raw(" "));
                    }
                    if *on {
                        spans.push(Span::styled("● ", Style::default().fg(*c)));
                        spans.push(Span::raw(text));
                    } else {
                        spans.push(Span::styled(format!("○ {text}"), muted));
                    }
                    spans.push(Span::raw(" ".repeat(pad + 1)));
                }
                Line::from(spans)
            })
            .collect()
    };
    let plain = Style::default();
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
            // Proteção: o que foi aplicado em pontos; pastas em colunas.
            let level = policy
                .as_ref()
                .and_then(|p| {
                    sandmux::launch::localize_summary(&p.level)
                        .split(':')
                        .next()
                        .map(str::to_string)
                })
                .unwrap_or_default();
            start(&mut secs, "protecao", tr!("Proteção", "Protection"), level);
            match policy {
                Some(p) => {
                    let mut items: Vec<(bool, String, Color)> = p
                        .level
                        .split_once(": ")
                        .map(|(_, rest)| {
                            rest.split(", ")
                                .map(|s| (true, s.replace("ABI ", ""), ok))
                                .collect()
                        })
                        .unwrap_or_default();
                    items.push((p.gpu, "GPU".into(), ok));
                    items.push((p.kvm, "KVM".into(), ok));
                    items.push((p.clipboard, "clipboard".into(), warn));
                    items.push((!p.pass_env.is_empty(), "pass-env".into(), warn));
                    for l in dots(&items) {
                        push(&mut secs, l);
                    }
                    let mut rows = row(
                        tr!("pasta", "folder"),
                        short(trf!("{} (escrita)", "{} (write)", p.workdir.display())),
                        plain,
                    );
                    for d in &p.ro {
                        rows.extend(row(
                            "extra",
                            short(trf!("{} (leitura)", "{} (read)", d.display())),
                            plain,
                        ));
                    }
                    for d in &p.rw {
                        rows.extend(row(
                            "extra",
                            short(trf!("{} (escrita)", "{} (write)", d.display())),
                            plain,
                        ));
                    }
                    rows.extend(row(
                        tr!("escrita", "write layer"),
                        sandmux::launch::localize_summary(&p.layer),
                        plain,
                    ));
                    if !p.pass_env.is_empty() {
                        rows.extend(row(
                            "pass-env",
                            p.pass_env.join(", "),
                            Style::default().fg(warn),
                        ));
                        rows.push(Line::styled(
                            format!(
                                "{:>w$}",
                                tr!("ao alcance do agente", "within the agent's reach")
                            ),
                            Style::default().fg(warn).add_modifier(Modifier::ITALIC),
                        ));
                    }
                    if !p.limits {
                        rows.extend(row(
                            tr!("tetos", "caps"),
                            tr!("sem suporte", "unsupported").into(),
                            Style::default().fg(warn),
                        ));
                    }
                    for l in rows {
                        push(&mut secs, l);
                    }
                }
                None => push(
                    &mut secs,
                    Line::styled(
                        tr!(" sem lançamento registrado", " no launch recorded"),
                        muted,
                    ),
                ),
            }

            // Recursos: colunas alinhadas, com medidores.
            start(
                &mut secs,
                "recursos",
                tr!("Recursos", "Resources"),
                state.to_string(),
            );
            match stats {
                Some(s) => {
                    let teto = |m: Option<u64>, f: &dyn Fn(u64) -> String| {
                        m.map(f).unwrap_or_else(|| tr!("sem teto", "no cap").into())
                    };
                    let bar = |used, max| {
                        let mut l = meter(used, max, w - 1, (ok, warn, bad), muted);
                        l.spans.insert(0, Span::raw(" "));
                        l
                    };
                    let mut rows = row(
                        tr!("memória", "memory"),
                        format!(
                            "{} / {}",
                            human(s.usage.memory),
                            teto(s.usage.memory_max, &human)
                        ),
                        plain,
                    );
                    if let Some(max) = s.usage.memory_max {
                        rows.push(bar(s.usage.memory, max));
                    }
                    rows.extend(row(tr!("pico", "peak"), human(s.memory_peak), plain));
                    rows.extend(row(
                        tr!("processos", "processes"),
                        format!(
                            "{} / {}",
                            s.usage.processes,
                            teto(s.usage.processes_max, &|n| n.to_string())
                        ),
                        plain,
                    ));
                    if let Some(max) = s.usage.processes_max {
                        rows.push(bar(s.usage.processes, max));
                    }
                    rows.extend(row(
                        "CPU",
                        format!("{}s", {
                            let cpu = format!("{:.1}", s.cpu_usec as f64 / 1e6);
                            if sandmux_spec::lang::is_en() {
                                cpu
                            } else {
                                cpu.replace('.', ",")
                            }
                        }),
                        plain,
                    ));
                    for l in rows {
                        push(&mut secs, l);
                    }
                }
                None => push(
                    &mut secs,
                    Line::styled(tr!(" sem consumo medido", " no usage measured"), muted),
                ),
            }
            let oom_style = if *oom_kills > 0 {
                Style::default().fg(bad).add_modifier(Modifier::BOLD)
            } else {
                plain
            };
            let mut rows = row(
                tr!("falta de memória", "out of memory"),
                oom_kills.to_string(),
                oom_style,
            );
            if let Some(t) = app.tabs.get(app.active) {
                let style = if t.keys.any() {
                    Style::default().fg(warn).add_modifier(Modifier::BOLD)
                } else {
                    plain
                };
                rows.extend(row(
                    tr!("chaves reais", "real keys"),
                    keys_text(&t.keys),
                    style,
                ));
            }
            match docker {
                Some(d) => {
                    rows.extend(row(
                        "Docker",
                        trf!(
                            "{} contêineres · {}",
                            "{} containers · {}",
                            d.containers,
                            human(d.memory)
                        ),
                        plain,
                    ));
                }
                None => rows.extend(row("Docker", tr!("parado", "stopped").into(), muted)),
            }
            for l in rows {
                push(&mut secs, l);
            }

            // Portas.
            start(
                &mut secs,
                "portas",
                tr!("Portas", "Ports"),
                match ports.len() {
                    0 => String::new(),
                    1 => tr!("1 publicada", "1 published").into(),
                    n => trf!("{n} publicadas", "{n} published"),
                },
            );
            if ports.is_empty() {
                push(
                    &mut secs,
                    Line::styled(tr!(" nenhuma publicada", " none published"), muted),
                );
            }
            for l in crate::sessions::port_lines(ports) {
                push(&mut secs, Line::raw(format!(" {l}")));
            }

            // Rede: tabela sem bordas (domínio, enviado, recebido, status).
            start(
                &mut secs,
                "rede",
                tr!("Rede", "Network"),
                policy
                    .as_ref()
                    .map(|p| {
                        sandmux::launch::localize_summary(&p.network)
                            .split(',')
                            .next()
                            .unwrap_or("")
                            .to_string()
                    })
                    .unwrap_or_default(),
            );
            if let Some(p) = policy {
                for l in row(
                    tr!("modo", "mode"),
                    sandmux::launch::localize_summary(&p.network),
                    plain,
                ) {
                    push(&mut secs, l);
                }
            }
            if app.panel_net.is_empty() {
                push(
                    &mut secs,
                    Line::styled(
                        tr!(" sem conexões registradas", " no connections logged"),
                        muted,
                    ),
                );
            } else {
                let dw = w.saturating_sub(18).max(6);
                push(
                    &mut secs,
                    Line::styled(
                        format!(
                            " {:<dw$} {:>5} {:>5} {:>4}",
                            tr!("domínio", "domain"),
                            "↑",
                            "↓",
                            "st"
                        ),
                        muted.add_modifier(Modifier::UNDERLINED),
                    ),
                );
            }
            for l in &app.panel_net {
                let (st, st_style) = match (&l.blocked, l.status, l.new) {
                    (Some(_), _, _) => ("✗".to_string(), Style::default().fg(bad)),
                    (None, Some(s), _) if s >= 400 => (s.to_string(), Style::default().fg(bad)),
                    (None, Some(s), _) => (s.to_string(), Style::default().fg(ok)),
                    (None, None, true) => {
                        (tr!("novo", "new").to_string(), Style::default().fg(warn))
                    }
                    _ => (String::new(), plain),
                };
                let dw = w.saturating_sub(18).max(6);
                let dom_style = if l.blocked.is_some() {
                    Style::default().fg(bad)
                } else if l.new {
                    Style::default().fg(warn).add_modifier(Modifier::BOLD)
                } else {
                    plain
                };
                // Domínio longo: numa linha só, e os números na de baixo.
                let dom = if l.domain.chars().count() > dw {
                    push(
                        &mut secs,
                        Line::styled(format!(" {}", cut(&l.domain, w - 1)), dom_style),
                    );
                    String::new()
                } else {
                    l.domain.clone()
                };
                push(
                    &mut secs,
                    Line::from(vec![
                        Span::styled(format!(" {dom:<dw$}"), dom_style),
                        Span::styled(format!(" {:>5} {:>5} ", human(l.up), human(l.down)), muted),
                        Span::styled(format!("{st:>4}"), st_style),
                    ]),
                );
                if let Some(b) = &l.blocked {
                    push(
                        &mut secs,
                        Line::styled(
                            trf!(
                                "   bloqueado: {}",
                                "   blocked: {}",
                                sandmux_spec::net::reason_text(b)
                            ),
                            Style::default().fg(bad),
                        ),
                    );
                }
                if l.new && l.status.is_some() {
                    push(
                        &mut secs,
                        Line::styled(tr!("   novo", "   new"), Style::default().fg(warn)),
                    );
                }
                if let Some(s) = &l.secret {
                    let what = if l.proto == "ssh-sign" {
                        tr!("assinatura ssh", "ssh signature")
                    } else {
                        tr!("chave", "key")
                    };
                    push(
                        &mut secs,
                        Line::styled(format!("   {what}: {s}"), Style::default().fg(accent)),
                    );
                }
            }

            // MCP por último: a rede, que muda a cada segundo, vem antes.
            let on = app.panel_mcp.iter().filter(|m| m.enabled).count();
            start(
                &mut secs,
                "mcp",
                "MCP",
                if app.panel_mcp.is_empty() {
                    String::new()
                } else {
                    trf!("{on} de {}", "{on} of {}", app.panel_mcp.len())
                },
            );
            if app.panel_mcp.is_empty() {
                push(
                    &mut secs,
                    Line::styled(tr!(" nenhum servidor", " no servers"), muted),
                );
            }
            for m in &app.panel_mcp {
                push(
                    &mut secs,
                    Line::from(vec![
                        if m.enabled {
                            Span::styled(" ✓ ", Style::default().fg(ok))
                        } else {
                            Span::styled(" ✗ ", Style::default().fg(bad))
                        },
                        if m.enabled {
                            Span::raw(m.name.clone())
                        } else {
                            Span::styled(m.name.clone(), muted)
                        },
                    ]),
                );
            }
        }
        _ => {
            start(
                &mut secs,
                "protecao",
                tr!("Proteção", "Protection"),
                String::new(),
            );
            push(
                &mut secs,
                Line::styled(tr!("carregando…", "loading…"), muted),
            );
        }
    }
    f.render_widget(Clear, r);
    let border = themed(theme, "modal-border", "", Style::default());
    f.render_widget(
        Block::default().borders(Borders::LEFT).border_style(border),
        r,
    );
    let mut hits = app.panel_hits.borrow_mut();
    hits.clear();
    let x0 = r.x + 2;
    let bottom = r.y + r.height;
    let mut y = r.y;
    let order: Vec<&str> = app.cfg.panel_order.iter().map(String::as_str).collect();
    let n = order.len();
    for (pos, key) in order.iter().enumerate() {
        let Some(sec) = secs.iter().find(|s| s.0 == *key) else {
            continue;
        };
        if y >= bottom {
            break;
        }
        if pos > 0 {
            y += 1;
            if y >= bottom {
                break;
            }
        }
        let collapsed = app.cfg.panel_collapsed.iter().any(|c| c == key);
        // Faixa: seta de recolher, título, resumo e as setas de ordem.
        let band = Style::default().bg(surface);
        let arrows = 4usize; // "↑ ↓ "
        let name = sec.1;
        let right = cut(&sec.2, w.saturating_sub(name.chars().count() + 4 + arrows));
        let pad = w.saturating_sub(name.chars().count() + right.chars().count() + 4 + arrows);
        let dim = |on: bool| {
            if on {
                band.fg(accent).add_modifier(Modifier::BOLD)
            } else {
                band.patch(muted)
            }
        };
        let line = Line::from(vec![
            Span::styled(if collapsed { "▸ " } else { "▾ " }, band.fg(accent)),
            Span::styled(
                format!("{name} "),
                band.fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ".repeat(pad), band),
            Span::styled(format!("{right} "), band.patch(muted)),
            Span::styled("↑", dim(pos > 0)),
            Span::styled(" ", band),
            Span::styled("↓", dim(pos + 1 < n)),
            Span::styled(" ", band),
        ]);
        f.render_widget(Paragraph::new(line), Rect::new(x0, y, w as u16, 1));
        let up_x = x0 + w as u16 - 4;
        hits.push((y, x0, up_x, super::PanelHit::Toggle(key.to_string())));
        hits.push((y, up_x, up_x + 2, super::PanelHit::Up(key.to_string())));
        hits.push((
            y,
            up_x + 2,
            up_x + 4,
            super::PanelHit::Down(key.to_string()),
        ));
        y += 1;
        if collapsed || y >= bottom {
            continue;
        }
        let p = Paragraph::new(sec.3.clone()).wrap(Wrap { trim: false });
        let h = (p.line_count(w as u16) as u16).min(bottom - y);
        f.render_widget(p, Rect::new(x0, y, w as u16, h));
        y += h;
    }
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
                    l.push(field(
                        c.field == 0,
                        tr!("pergunta", "question"),
                        format!("{} ", c.question),
                    ));
                    l.push(field(
                        c.field == 1,
                        tr!("modelo", "model"),
                        format!("{} ", c.model),
                    ));
                    l.push(field(
                        c.field == 2,
                        tr!("tela", "screen"),
                        if c.screen {
                            trf!(
                                "● enviar as últimas {} linhas da aba ",
                                "● send the tab's last {} lines ",
                                super::SCREEN_LINES
                            )
                        } else {
                            tr!("○ não enviar ", "○ do not send ").to_string()
                        },
                    ));
                    if c.screen {
                        l.push(Line::styled(
                            tr!(
                                "a tela da aba vai ao provedor junto com a pergunta",
                                "the tab's screen goes to the provider with the question"
                            ),
                            error,
                        ));
                    }
                    l.push(Line::raw(""));
                    l.push(Line::raw(tr!(
                        "Tab troca de campo · ↑ ↓ modelos · → lista dos provedores · Espaço na tela liga · Enter pergunta · Esc cancela",
                        "Tab switches field · ↑ ↓ models · → providers' list · Space on screen toggles · Enter asks · Esc cancels"
                    )));
                }
                CmdStage::Waiting => {
                    l.push(Line::raw(trf!("pergunta  {}", "question  {}", c.question)));
                    l.push(Line::raw(trf!("modelo    {}", "model     {}", c.model)));
                    l.push(Line::raw(""));
                    l.push(Line::raw(tr!(
                        "esperando o modelo… (Esc cancela)",
                        "waiting for the model… (Esc cancels)"
                    )));
                }
                CmdStage::Command => {
                    let insert = tr!("[ Inserir ]", "[ Insert ]");
                    button = Some((0, 0, insert.chars().count() as u16));
                    l.push(Line::from(vec![
                        Span::styled(insert, selected),
                        Span::raw(tr!(
                            "  escreve na aba, sem executar",
                            "  types it into the tab, without running"
                        )),
                    ]));
                    l.push(Line::raw(""));
                    for (i, line) in c.command.split('\n').enumerate() {
                        let label = if i == 0 {
                            tr!("comando", "command")
                        } else {
                            ""
                        };
                        l.push(Line::from(vec![
                            Span::raw(format!("{label:<10}")),
                            Span::styled(
                                line.to_string(),
                                Style::default().add_modifier(Modifier::BOLD),
                            ),
                        ]));
                    }
                    l.push(Line::raw(""));
                    l.push(Line::raw(tr!(
                        "Enter insere · edite antes, se quiser · Tab refaz a pergunta · Esc cancela",
                        "Enter inserts · edit it first if you like · Tab redoes the question · Esc cancels"
                    )));
                }
            }
            if let Some(e) = &c.error {
                l.push(Line::styled(e.clone(), error));
            }
            let h = l.len() as u16 + 2;
            (tr!("Comando", "Command"), l, h)
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
                field(1, tr!("pasta", "folder"), format!("{} ", n.workdir)),
                field(2, tr!("perfil", "profile"), format!("{} ", n.profile)),
                Line::raw(""),
            ];
            if let Some(q) = &n.question {
                l.push(Line::raw(q.clone()));
            } else {
                l.push(Line::raw(tr!(
                    "Tab troca de campo · ← → harness · Enter cria · Esc cancela",
                    "Tab switches field · ← → harness · Enter creates · Esc cancels"
                )));
            }
            if let Some(e) = &n.error {
                l.push(Line::styled(e.clone(), error));
            }
            (tr!("Nova aba", "New tab"), l, 10)
        }
        Modal::Ended(id, code) => {
            let name = app
                .tabs
                .iter()
                .find(|t| t.id == *id)
                .map(|t| t.name.clone())
                .unwrap_or_default();
            (
                tr!("Aba encerrada", "Tab ended"),
                vec![
                    Line::raw(trf!(
                        "{name} terminou com o código {code}.",
                        "{name} exited with code {code}."
                    )),
                    Line::raw(""),
                    Line::raw(tr!(
                        "Enter ou r reinicia · x fecha a aba · Esc deixa como está",
                        "Enter or r restarts · x closes the tab · Esc leaves it as is"
                    )),
                ],
                5,
            )
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
                field(0, tr!("mensagem", "message"), format!("{} ", c.text)),
                field(1, tr!("modelo", "model"), format!("{} ", c.model)),
                Line::raw(""),
                Line::raw(tr!(
                    "Tab troca de campo · ↑ ↓ modelos da configuração · → lista dos provedores · Enter envia · Esc cancela",
                    "Tab switches field · ↑ ↓ configured models · → providers' list · Enter sends · Esc cancels"
                )),
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
            (
                tr!(
                    "Abas (↑ ↓ Enter, Esc fecha)",
                    "Tabs (↑ ↓ Enter, Esc closes)"
                ),
                l,
                h,
            )
        }
        Modal::Rename(text) => (
            tr!(
                "Renomear aba (Enter grava, Esc cancela)",
                "Rename tab (Enter saves, Esc cancels)"
            ),
            vec![Line::raw(format!("{text} "))],
            3,
        ),
        Modal::Close => (
            tr!("Fechar aba", "Close tab"),
            vec![Line::raw(tr!(
                "Encerrar a sessão desta aba? s/n",
                "End this tab's session? y/n"
            ))],
            3,
        ),
        Modal::Mcp(ms) => {
            let mut l = vec![Line::raw(trf!(
                "Servidores MCP da sessão {} (valem no próximo lançamento)",
                "MCP servers of session {} (take effect on the next launch)",
                ms.id
            ))];
            if ms.list.is_empty() {
                l.push(Line::raw(tr!("nenhum servidor", "no servers")));
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
            l.push(Line::raw(tr!(
                "espaço habilita/desabilita · ↑ ↓ · Esc fecha",
                "space enables/disables · ↑ ↓ · Esc closes"
            )));
            if let Some(n) = &ms.note {
                l.push(Line::raw(n.clone()));
            }
            let h = l.len() as u16 + 2;
            ("MCP", l, h)
        }
        Modal::Folders(fs) => {
            let mut l = vec![Line::raw(trf!(
                "Pastas da sessão {} em /mnt/sandmux (valem na hora)",
                "Folders of session {} in /mnt/sandmux (take effect right away)",
                fs.id
            ))];
            if fs.list.is_empty() {
                l.push(Line::raw(tr!("nenhuma pasta", "no folders")));
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
                    trf!(
                        "pasta: {t}▏ (Enter confirma, Esc cancela)",
                        "folder: {t}▏ (Enter confirms, Esc cancels)"
                    ),
                    selected,
                ),
                (_, Some(p)) => Line::styled(
                    trf!(
                        "{p}: r só leitura · w leitura e escrita · Esc cancela",
                        "{p}: r read-only · w read-write · Esc cancels"
                    ),
                    selected,
                ),
                _ => Line::raw(tr!(
                    "a monta uma pasta · x desmonta · ↑ ↓ · Esc fecha",
                    "a mounts a folder · x unmounts · ↑ ↓ · Esc closes"
                )),
            });
            if let Some(n) = &fs.note {
                l.push(Line::raw(n.clone()));
            }
            let h = l.len() as u16 + 2;
            (tr!("Pastas", "Folders"), l, h)
        }
        Modal::Files(fs) => {
            let mut l = vec![Line::raw(trf!(
                "Acessos a arquivos da sessão {} (• nunca visto nesta pasta)",
                "File accesses of session {} (• never seen in this folder)",
                fs.id
            ))];
            let room = full.height.saturating_sub(10).max(3) as usize;
            let start = fs.sel.saturating_sub(room.saturating_sub(1));
            if fs.lines.is_empty() {
                l.push(Line::raw(tr!(
                    "sem acessos registrados desde ontem",
                    "no accesses logged since yesterday"
                )));
            }
            for (i, x) in fs.lines.iter().enumerate().skip(start).take(room) {
                let hora = x.ts.split('T').nth(1).unwrap_or(&x.ts);
                let nota = x
                    .blocked
                    .as_deref()
                    .map(|b| trf!(" bloqueado: {b}", " blocked: {b}"))
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
            l.push(Line::raw(tr!(
                "b bloqueia · l libera · ↑ ↓ · Esc fecha",
                "b blocks · l allows · ↑ ↓ · Esc closes"
            )));
            if let Some(n) = &fs.note {
                l.push(Line::raw(n.clone()));
            }
            let h = l.len() as u16 + 2;
            (tr!("Arquivos", "Files"), l, h)
        }
        Modal::Net(ns) => {
            let mut l = vec![Line::raw(trf!(
                "Modo: {}; {}   (m troca o modo da aba)",
                "Mode: {}; {}   (m switches the tab mode)",
                ns.mode.describe(),
                sandmux_policy::config::origin_text(&ns.mode_origin)
            ))];
            let tab = |i: usize, label: &str| {
                if ns.list == i {
                    Span::styled(format!(" {label} "), selected)
                } else {
                    Span::raw(format!(" {label} "))
                }
            };
            l.push(Line::from(vec![
                tab(0, tr!("Acessos", "Accesses")),
                Span::raw(" "),
                tab(1, tr!("Regras", "Rules")),
                Span::raw(tr!("   (Tab alterna)", "   (Tab switches)")),
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
                            (Some(b), _) => trf!(
                                "bloqueado: {}",
                                "blocked: {}",
                                sandmux_spec::net::reason_text(b)
                            ),
                            (None, true) => tr!("novo", "new").into(),
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
                            if r.enabled {
                                tr!("ligado", "on")
                            } else {
                                tr!("desligado", "off")
                            },
                            match r.kind {
                                sandmux_spec::net::RuleKind::Deny => tr!("nega", "deny"),
                                sandmux_spec::net::RuleKind::Allow => tr!("libera", "allow"),
                            },
                            r.pattern.to_string(),
                            sandmux_policy::config::origin_text(&r.origin)
                        )
                    })
                    .collect()
            };
            if rows.is_empty() {
                l.push(Line::raw(if ns.list == 0 {
                    tr!(
                        "sem acessos registrados desde ontem",
                        "no accesses logged since yesterday"
                    )
                } else {
                    tr!("sem regras", "no rules")
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
                None if ns.list == 0 => Line::raw(tr!(
                    "n nega · l libera · ↑ ↓ · Esc fecha",
                    "n denies · l allows · ↑ ↓ · Esc closes"
                )),
                None => Line::raw(tr!(
                    "espaço liga/desliga · x tira a regra · ↑ ↓ · Esc fecha",
                    "space turns on/off · x removes the rule · ↑ ↓ · Esc closes"
                )),
            });
            if let Some(n) = &ns.note {
                l.push(Line::styled(n.clone(), error));
            }
            let h = l.len() as u16 + 2;
            (tr!("Rede da aba", "Tab network"), l, h)
        }
        Modal::Unlock(text, err) => {
            let mut l = vec![
                Line::raw(tr!(
                    "O cofre de segredos está trancado.",
                    "The secrets vault is locked."
                )),
                Line::raw(trf!(
                    "Senha: {}",
                    "Password: {}",
                    "•".repeat(text.chars().count())
                )),
                Line::raw(""),
                Line::raw(tr!(
                    "Enter desbloqueia · Esc deixa trancado",
                    "Enter unlocks · Esc leaves it locked"
                )),
            ];
            if let Some(e) = err {
                l.push(Line::styled(e.clone(), error));
            }
            let h = l.len() as u16 + 2;
            (tr!("Cofre", "Vault"), l, h)
        }
        Modal::Menu(sel) => {
            let p = app.prefix_name();
            let l: Vec<Line> = super::menu_items()
                .iter()
                .enumerate()
                .map(|(i, (action, label))| {
                    let row = format!(" {p} {}   {label} ", key_label(app.cfg.key(action)));
                    if i == *sel {
                        Line::styled(row, selected)
                    } else {
                        Line::raw(row)
                    }
                })
                .collect();
            let h = l.len() as u16 + 2;
            (
                tr!(
                    "Menu (↑ ↓ Enter, Esc fecha)",
                    "Menu (↑ ↓ Enter, Esc closes)"
                ),
                l,
                h,
            )
        }
        Modal::Settings(_) => return,
        Modal::Help => {
            let p = app.prefix_name();
            let mut l = vec![Line::raw(trf!("Tecla de prefixo: {p}", "Prefix key: {p}"))];
            for item in super::HELP {
                let label = super::help_label(item);
                l.push(Line::raw(format!(
                    "  {p} {}   {label}",
                    key_label(app.cfg.key(item.0))
                )));
            }
            l.push(Line::raw(trf!(
                "  {p} 1..9  ir para a aba",
                "  {p} 1..9  go to the tab"
            )));
            l.push(Line::raw(trf!(
                "  {p} {p}  envia o prefixo ao harness",
                "  {p} {p}  sends the prefix to the harness"
            )));
            let h = l.len() as u16 + 2;
            (tr!("Ajuda (Esc fecha)", "Help (Esc closes)"), l, h)
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
        .set(button.map(|(li, col, w)| (r.x + 2 + col, r.y + 1 + li, w)));
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .padding(Padding::horizontal(1))
                .title(format!(" {title} "))
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
    fn medidor_de_uso() {
        let c = (Color::Green, Color::Yellow, Color::Red);
        let text = |l: &Line| {
            l.spans
                .iter()
                .map(|s| s.content.to_string())
                .collect::<String>()
        };
        let l = meter(5, 100, 10, c, Style::default());
        assert_eq!(text(&l), "█░░░░░░░░░");
        assert_eq!(l.spans[0].style.fg, Some(Color::Green));
        let l = meter(80, 100, 10, c, Style::default());
        assert_eq!(text(&l), "████████░░");
        assert_eq!(l.spans[0].style.fg, Some(Color::Yellow));
        let l = meter(99, 100, 10, c, Style::default());
        assert_eq!(l.spans[0].style.fg, Some(Color::Red));
        assert_eq!(text(&meter(0, 100, 4, c, Style::default())), "░░░░");
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
