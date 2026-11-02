//! Popup de ajuda (`?`): 60×20 centralizado com todas as teclas de
//! ARCHITECTURE §6.4 em duas colunas, a ordem dos painéis e os glifos.
//! Qualquer tecla fecha (ver `input.rs`).

use peabiru_core::NodeStatus;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, Panel};
use crate::theme::Theme;
use crate::widgets::{centered_rect, panel_block, status_glyph};

/// Tamanho do popup.
pub const WIDTH: u16 = 60;
pub const HEIGHT: u16 = 20;

/// Teclas de §6.4 como `(tecla, descrição)`, na ordem da tabela. Também é a
/// fonte do KEYMAP completo do Tab Settings.
pub const KEYS: &[(&str, &str)] = &[
    ("1-5", "switch tab"),
    ("?", "toggle help"),
    ("q  ^C", "quit"),
    ("r", "run trace"),
    ("Tab", "next panel"),
    ("S-Tab", "previous panel"),
    ("↑↓ kj", "navigate"),
    ("Enter", "select / details"),
    ("d", "node details"),
    ("y", "yolo mode"),
    ("p", "pause events"),
    ("Esc", "close / trace tab"),
    ("g G", "first / last"),
];

/// Largura da coluna de tecla nas linhas de `key_entry`.
const KEY_COL: usize = 7;

/// `" {tecla:<7} {descrição}"`: tecla em `key`, descrição em `text`,
/// preenchida até `width` colunas.
pub fn key_entry<'a>(theme: &Theme, key: &str, desc: &str, width: usize) -> Vec<Span<'a>> {
    let desc_w = width.saturating_sub(KEY_COL + 2);
    vec![
        Span::styled(format!(" {key:<KEY_COL$} "), theme.key()),
        Span::styled(format!("{desc:<desc_w$}"), theme.text()),
    ]
}

/// `area` é a tela inteira; o popup se centraliza sozinho.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let rect = centered_rect(WIDTH, HEIGHT, area);
    frame.render_widget(Clear, rect);
    let block = panel_block(theme, "HELP", true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.height < 2 || inner.width < 10 {
        return;
    }
    let [body, footer] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);

    let col_w = usize::from(inner.width) / 2;
    let rows = KEYS.len().div_ceil(2);
    let mut lines: Vec<Line> = vec![Line::default(), section(theme, "KEYS")];
    for row in 0..rows {
        let mut spans = Vec::new();
        for col in 0..2 {
            if let Some((key, desc)) = KEYS.get(col * rows + row) {
                spans.extend(key_entry(theme, key, desc, col_w));
            }
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::default());
    lines.push(section(theme, "PANELS  (Tab / Shift-Tab order)"));
    let titles: Vec<&str> = Panel::ORDER.iter().map(Panel::title).collect();
    let (first, rest) = titles.split_at(titles.len().min(3));
    lines.push(Line::from(Span::styled(
        format!("  {} →", first.join(" → ")),
        theme.text(),
    )));
    lines.push(Line::from(Span::styled(
        format!("  {}", rest.join(" → ")),
        theme.text(),
    )));

    lines.push(Line::default());
    lines.push(section(theme, "GLYPHS"));
    let mut glyphs = vec![Span::raw(" ")];
    for (status, name) in [
        (NodeStatus::Observed, "observed"),
        (NodeStatus::Inferred, "inferred"),
        (NodeStatus::Correlated, "correlated"),
        (NodeStatus::Probe, "probe"),
    ] {
        glyphs.push(Span::styled(
            format!(" {}", status_glyph(status)),
            theme.status(status),
        ));
        glyphs.push(Span::styled(format!(" {name} "), theme.text()));
    }
    lines.push(Line::from(glyphs));

    frame.render_widget(Paragraph::new(lines), body);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "press any key to close",
            theme.muted(),
        )))
        .alignment(Alignment::Center),
        footer,
    );
}

fn section<'a>(theme: &Theme, title: &str) -> Line<'a> {
    Line::from(Span::styled(
        format!(" {title}"),
        theme.title().add_modifier(Modifier::BOLD),
    ))
}
