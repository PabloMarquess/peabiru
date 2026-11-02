//! Painel TRACE TARGET (ARCHITECTURE §6.2): alvo, porta/protocolo, origem,
//! separador e a ação `[R] Run Trace` (ou o spinner `tracing…` em `Running`).

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Padding, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Panel};
use crate::theme::Theme;
use crate::widgets::{panel_block, spinner};

/// Espaço entre a borda e o texto, de cada lado (como no mockup).
pub const PADDING: u16 = 1;

/// Reticências usadas na truncagem.
pub const ELLIPSIS: char = '…';

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let block = panel_block(theme, "TRACE TARGET", app.focus == Panel::TraceTarget)
        .padding(Padding::horizontal(PADDING));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let width = usize::from(inner.width);
    let req = &app.request;

    let target = Line::from(Span::styled(
        fit(&req.to, width),
        theme.text().add_modifier(Modifier::BOLD),
    ));
    let port = labeled(
        theme,
        "PORT: ",
        &format!("{} ({})", req.port, req.protocol),
        width,
    );
    let from = labeled(theme, "FROM: ", &req.from, width);
    let separator = Line::from(Span::styled(
        "─".repeat(width),
        Style::default().fg(theme.border),
    ));
    let action = if app.run.is_running() {
        Line::from(Span::styled(
            fit(&format!("{} tracing…", spinner(app.tick)), width),
            Style::default().fg(theme.warn),
        ))
    } else {
        let key = "[R]";
        let label = fit(" Run Trace", width.saturating_sub(key.width()));
        Line::from(vec![
            Span::styled(key, theme.key()),
            Span::styled(label, Style::default().fg(theme.observed)),
        ])
    };

    // A altura de referência do painel (7, com bordas) dá 5 linhas úteis;
    // com menos, o separador é o primeiro a cair (a ação fica).
    let mut lines = vec![target, port, from];
    if usize::from(inner.height) >= 5 {
        lines.push(separator);
    }
    lines.push(action);
    lines.truncate(usize::from(inner.height));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// `KEY: value`: chave em `muted`, valor em `text`, truncado para `width`.
fn labeled<'a>(theme: &Theme, key: &'a str, value: &str, width: usize) -> Line<'a> {
    let value = fit(value, width.saturating_sub(key.width()));
    Line::from(vec![
        Span::styled(key, theme.muted()),
        Span::styled(value, theme.text()),
    ])
}

/// Trunca `text` para caber em `width` colunas (largura Unicode), terminando
/// em `…` quando cortar. `width == 0` devolve vazio; com `1` sobra só `…`.
pub fn fit(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let budget = width - ELLIPSIS.width().unwrap_or(1);
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push(ELLIPSIS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_short_and_truncates_long() {
        assert_eq!(fit("abc", 3), "abc");
        assert_eq!(fit("abc", 10), "abc");
        assert_eq!(fit("abcdef", 4), "abc…");
        assert_eq!(fit("abcdef", 1), "…");
        assert_eq!(fit("abcdef", 0), "");
        assert_eq!(fit("", 0), "");
        // caractere largo não é cortado ao meio
        assert_eq!(fit("コンニチハ", 4), "コ…");
        assert_eq!(fit("コン", 4), "コン");
    }
}
