//! Seletor de modelo: a lista dos provedores do assistente (`ListModels` no
//! daemon, que pede `/models` a cada um dentro do sandbox da aba de chat),
//! com filtro por digitação. Fica por cima dos modais.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};
use sandmux_proto::Msg;
use sandmux_spec::{tr, trf};

use super::input::Key;
use super::render::{cut, pl};
use super::{App, Ev, Modal};

/// Para onde vai o modelo escolhido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickFor {
    ChatModal,
    CommandModal,
    /// Grava o item da página de configurações (`chat-model`,
    /// `command-model`).
    Setting,
    /// Acrescenta à lista `models` da página.
    SettingAdd,
}

/// A lista de modelos, guardada até a TUI fechar.
#[derive(Debug, Clone, Default)]
pub enum Models {
    #[default]
    None,
    Loading,
    /// Os modelos e os avisos dos provedores que falharam.
    Ready(Vec<String>, Vec<String>),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct Picker {
    pub to: PickFor,
    pub filter: String,
    pub sel: usize,
}

/// Os modelos com todas as palavras do filtro, sem caixa.
pub fn filtered<'a>(all: &'a [String], filter: &str) -> Vec<&'a String> {
    sandmux::chat::filter_models(all, filter)
}

impl App {
    /// Abre o seletor; a lista é pedida na primeira vez (ou depois de uma
    /// falha).
    pub(super) fn open_picker(&mut self, to: PickFor) {
        self.picker = Some(Picker {
            to,
            filter: String::new(),
            sel: 0,
        });
        if matches!(self.models, Models::None | Models::Failed(_)) {
            self.load_models();
        }
    }

    /// Pede a lista ao daemon numa thread, com uma conexão própria.
    pub(super) fn load_models(&mut self) {
        self.models = Models::Loading;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let r = sandmux::client::connect_or_start().and_then(|mut c| c.call(&Msg::ListModels));
            let r = match r {
                Ok(Msg::Models { ids, notes }) => Ok((ids, notes)),
                Ok(_) => Err(tr!(
                    "resposta inesperada do daemon",
                    "unexpected answer from the daemon"
                )
                .to_string()),
                Err(e) => Err(format!("{e:#}")),
            };
            let _ = tx.send(Ev::Models(r));
        });
    }

    pub(super) fn models_loaded(&mut self, r: Result<(Vec<String>, Vec<String>), String>) {
        self.models = match r {
            Ok((ids, notes)) => Models::Ready(ids, notes),
            Err(e) => Models::Failed(e),
        };
    }

    pub(super) fn picker_key(&mut self, k: Key) {
        let Some(p) = self.picker.as_mut() else {
            return;
        };
        let list: Vec<String> = match &self.models {
            Models::Ready(all, _) => filtered(all, &p.filter).into_iter().cloned().collect(),
            _ => Vec::new(),
        };
        match k {
            Key::Esc => self.picker = None,
            Key::Up => p.sel = p.sel.saturating_sub(1),
            Key::Down => p.sel = (p.sel + 1).min(list.len().saturating_sub(1)),
            Key::Backspace => {
                p.filter.pop();
                p.sel = 0;
            }
            Key::Char(c) => {
                p.filter.push(c);
                p.sel = 0;
            }
            Key::Ctrl('r') => self.load_models(),
            Key::Enter => {
                // Sem item na lista, vale o texto digitado.
                let chosen = list
                    .get(p.sel)
                    .cloned()
                    .unwrap_or_else(|| p.filter.trim().to_string());
                let to = p.to;
                self.picker = None;
                if !chosen.is_empty() {
                    self.picked(to, chosen);
                }
            }
            _ => {}
        }
    }

    fn picked(&mut self, to: PickFor, model: String) {
        match to {
            PickFor::ChatModal => {
                if let Some(Modal::Chat(c)) = &mut self.modal {
                    c.model = model;
                    c.choice = None;
                }
            }
            PickFor::CommandModal => {
                if let Some(Modal::Command(c)) = &mut self.modal {
                    c.model = model;
                    c.choice = None;
                }
            }
            PickFor::Setting | PickFor::SettingAdd => {
                if let Some(Modal::Settings(mut s)) = self.modal.take() {
                    self.settings_pick(&mut s, to == PickFor::SettingAdd, model);
                    self.modal = Some(Modal::Settings(s));
                }
            }
        }
    }
}

/// Desenha o seletor no meio da tela.
pub fn draw(f: &mut Frame, full: Rect, app: &App) {
    let Some(p) = &app.picker else {
        return;
    };
    let theme = if app.cfg.theme.colors.is_empty() {
        sandmux_policy::config::Theme::builtin("escuro").unwrap_or_default()
    } else {
        app.cfg.theme.clone()
    };
    let c = |el: &str| pl(&theme, el, "default");
    let (bg, fg, surface, accent) = (c("page-bg"), c("page-fg"), c("surface-bg"), c("accent"));
    let (muted, err, on_accent) = (c("muted-fg"), c("error-fg"), c("selected-fg"));
    let base = Style::default().fg(fg).bg(bg);
    let w = full.width.saturating_sub(4).min(84);
    let h = full.height.saturating_sub(4).min(26);
    let r = Rect::new(
        full.x + (full.width - w) / 2,
        full.y + (full.height - h) / 2,
        w,
        h,
    );
    f.render_widget(Clear, r);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent).bg(bg))
        .padding(Padding::horizontal(1))
        .style(base)
        .title(Line::styled(
            tr!(" Escolher o modelo ", " Choose the model "),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::styled(
            tr!(
                " ↑ ↓ · Enter escolhe · Ctrl+r relê · Esc cancela ",
                " ↑ ↓ · Enter picks · Ctrl+r reloads · Esc cancels "
            ),
            Style::default().fg(muted),
        ));
    let inner = block.inner(r);
    f.render_widget(block, r);
    if inner.height < 4 {
        return;
    }
    let iw = inner.width as usize;
    let put = |f: &mut Frame, y: u16, line: Line| {
        f.render_widget(
            Paragraph::new(line).style(base),
            Rect::new(inner.x, y, inner.width, 1),
        );
    };
    put(
        f,
        inner.y,
        Line::from(vec![
            Span::styled(tr!("filtro: ", "filter: "), Style::default().fg(muted)),
            Span::styled(
                format!("{}▏", p.filter),
                Style::default()
                    .fg(fg)
                    .bg(surface)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    );
    let mut y = inner.y + 1;
    let (list, status, notes): (Vec<String>, Line, &[String]) = match &app.models {
        Models::None | Models::Loading => (
            Vec::new(),
            Line::styled(
                tr!(
                    "carregando os modelos dos provedores…",
                    "loading the providers' models…"
                ),
                Style::default().fg(muted),
            ),
            &[],
        ),
        Models::Failed(e) => (
            Vec::new(),
            Line::styled(
                trf!(
                    "não deu para listar: {e}. Enter usa o texto digitado.",
                    "could not list: {e}. Enter uses the typed text."
                ),
                Style::default().fg(err),
            ),
            &[],
        ),
        Models::Ready(all, notes) => {
            let l: Vec<String> = filtered(all, &p.filter).into_iter().cloned().collect();
            let n = l.len();
            let line = Line::styled(
                if n == 0 {
                    tr!(
                        "nenhum modelo com esse filtro; Enter usa o texto digitado",
                        "no model matches; Enter uses the typed text"
                    )
                    .to_string()
                } else {
                    trf!("{n} de {}", "{n} of {}", all.len())
                },
                Style::default().fg(muted),
            );
            (l, line, notes.as_slice())
        }
    };
    put(f, y, status);
    y += 1;
    for n in notes.iter().take(2) {
        put(
            f,
            y,
            Line::styled(cut(&format!("! {n}"), iw), Style::default().fg(err)),
        );
        y += 1;
    }
    y += 1;
    let bottom = inner.y + inner.height;
    let room = bottom.saturating_sub(y) as usize;
    if room == 0 {
        return;
    }
    let sel = p.sel.min(list.len().saturating_sub(1));
    let start = sel.saturating_sub(room.saturating_sub(1));
    for (i, m) in list.iter().enumerate().skip(start).take(room) {
        let st = if i == sel {
            Style::default()
                .fg(on_accent)
                .bg(accent)
                .add_modifier(Modifier::BOLD)
        } else {
            base
        };
        put(
            f,
            y,
            Line::styled(format!(" {} ", cut(m, iw.saturating_sub(2))), st),
        );
        y += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtro_por_palavras() {
        let all = vec![
            "openrouter/deepseek/deepseek-v4.1-flash-1017".to_string(),
            "openrouter/openai/gpt-5".to_string(),
            "openai/gpt-5-mini".to_string(),
        ];
        assert_eq!(filtered(&all, "").len(), 3);
        assert_eq!(filtered(&all, "FLASH"), vec![&all[0]]);
        assert_eq!(filtered(&all, "gpt openrouter"), vec![&all[1]]);
        assert!(filtered(&all, "claude").is_empty());
    }
}
