//! Painel KEYMAP (ARCHITECTURE §6.2): atalhos como `tecla  descrição`.
//!
//! Quando a lista não cabe na altura mas cabe na largura, é dividida em
//! duas colunas balanceadas; senão mostra as primeiras que cabem.

use ratatui::layout::{Margin, Rect};
use ratatui::text::{Line, Span};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::ui::events::pad_to;
use crate::widgets::panel_block;

/// Largura da coluna de tecla (`enter` + 1).
pub const KEY_COL: usize = 6;
/// Espaço entre as duas colunas de atalhos.
const GAP: usize = 4;

/// Atalhos exibidos (tecla, descrição), na ordem do painel.
pub const BINDINGS: [(&str, &str); 8] = [
    ("r", "run trace"),
    ("tab", "next panel"),
    ("↑/↓", "navigate"),
    ("enter", "select"),
    ("d", "node details"),
    ("y", "yolo mode"),
    ("p", "pause events"),
    ("q", "quit"),
];

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let block = panel_block(theme, "KEYMAP", false);
    let inner = block.inner(area).inner(Margin::new(1, 0));
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let rows = usize::from(inner.height);
    let col_w = BINDINGS
        .iter()
        .map(|(_, desc)| KEY_COL + desc.width())
        .max()
        .unwrap_or(KEY_COL);
    let two_columns = BINDINGS.len() > rows && usize::from(inner.width) >= 2 * col_w + GAP;
    let per_column = if two_columns {
        BINDINGS.len().div_ceil(2)
    } else {
        BINDINGS.len()
    };

    let buf = frame.buffer_mut();
    for (ci, chunk) in BINDINGS.chunks(per_column).enumerate() {
        let offset = u16::try_from(ci * (col_w + GAP)).unwrap_or(u16::MAX);
        let x = inner.x.saturating_add(offset);
        if x >= inner.right() {
            break;
        }
        for ((key, desc), y) in chunk.iter().zip(inner.y..inner.bottom()) {
            let line = Line::from(vec![
                Span::styled(pad_to(key, KEY_COL), theme.key()),
                Span::styled(*desc, theme.text()),
            ]);
            buf.set_line(x, y, &line, inner.right() - x);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_fit_their_column() {
        for (key, desc) in BINDINGS {
            assert!(key.width() < KEY_COL, "{key}");
            assert!(!desc.is_empty());
        }
    }
}
