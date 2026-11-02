//! Painel EVENTS (ARCHITECTURE §6.2): stream de eventos do sensor.
//!
//! Colunas: hora local (UTC com `App::fixed_clock`), glifo + fonte (cor pelo `source.status()`), action,
//! from, seta, to e detail alinhado à direita. As larguras de `from`/`to`
//! vêm dos próprios eventos e se adaptam à largura do painel; abaixo de
//! `DETAIL_MIN_WIDTH` colunas de texto a coluna `detail` some. Sem pausa o
//! painel mostra as últimas N linhas (auto-scroll); pausado, a janela segue
//! `selected_event`.

use peabiru_core::Event;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Panel};
use crate::theme::Theme;
use crate::widgets::{fmt_clock, panel_block_with_right, status_glyph};

/// `HH:MM:SS`.
const TIME_WIDTH: usize = 8;
/// Glifo + espaço.
const GLYPH_WIDTH: usize = 2;
/// `correlation` (11) + 1.
const SOURCE_WIDTH: usize = 12;
/// `connect`/`resolve` (7) + 1.
const ACTION_WIDTH: usize = 8;
/// Colunas fixas antes de `from`: hora, glifo, fonte, action e os espaços.
const FIXED_WIDTH: usize = TIME_WIDTH + 1 + GLYPH_WIDTH + SOURCE_WIDTH + 1 + ACTION_WIDTH + 1;
/// ` → ` / ` ↔ `.
const ARROW_WIDTH: usize = 3;
/// Largura de texto mínima para exibir a coluna `detail`.
const DETAIL_MIN_WIDTH: usize = 90;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let focused = app.focus == Panel::Events;
    let block = panel_block_with_right(
        theme,
        "EVENTS",
        pause_hint(theme, app.events_paused),
        focused,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let text = inner.inner(Margin::new(1, 0));
    if text.width == 0 || text.height == 0 {
        return;
    }

    if app.events.is_empty() {
        let msg = Line::from(vec![
            Span::styled("waiting for events… press ", theme.muted()),
            Span::styled("r", theme.key()),
            Span::styled(" to run a trace", theme.muted()),
        ]);
        frame.render_widget(Paragraph::new(msg), text);
        return;
    }

    let n = app.events.len();
    let rows = usize::from(text.height);
    let selected = app.selected_event.min(n - 1);
    let (start, end) = window(n, rows, selected, app.events_paused);
    let highlight = (app.events_paused || focused).then_some(selected);
    let columns = Columns::fit(&app.events, usize::from(text.width));
    let utc = app.fixed_clock.is_some();

    let buf = frame.buffer_mut();
    for ((i, ev), y) in app.events[start..end]
        .iter()
        .enumerate()
        .zip(text.y..text.bottom())
    {
        if highlight == Some(start + i) {
            buf.set_style(Rect::new(inner.x, y, inner.width, 1), theme.selection());
        }
        buf.set_line(text.x, y, &columns.line(theme, ev, utc), text.width);
    }
}

/// `[p] pause` (tecla em `key`) ou `[p] resume` em `warn` quando pausado.
fn pause_hint<'a>(theme: &Theme, paused: bool) -> Line<'a> {
    if paused {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "[p] resume",
                Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("[p]", theme.key()),
            Span::styled(" pause", theme.muted()),
            Span::raw(" "),
        ])
    }
}

/// Janela `[start, end)` de `rows` linhas sobre `n` eventos: as últimas
/// (auto-scroll) ou, pausado, a que contém `selected`.
fn window(n: usize, rows: usize, selected: usize, paused: bool) -> (usize, usize) {
    if n == 0 || rows == 0 {
        return (0, 0);
    }
    let end = if paused {
        (selected + 1).max(rows).min(n)
    } else {
        n
    };
    (end.saturating_sub(rows), end)
}

/// Larguras das colunas variáveis, calculadas sobre todos os eventos (as
/// colunas não pulam quando o stream rola).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Columns {
    /// Largura de texto do painel.
    width: usize,
    /// Coluna `from` (preenchida com espaços).
    from: usize,
    /// Coluna `to`; `0` = sem preenchimento (o detail já é alinhado à direita).
    to: usize,
    /// Coluna `detail` visível.
    detail: bool,
}

impl Columns {
    fn fit(events: &[Event], width: usize) -> Self {
        let detail = width >= DETAIL_MIN_WIDTH;
        let widest = |f: fn(&Event) -> &str| events.iter().map(|e| f(e).width()).max().unwrap_or(0);
        let from_max = widest(|e| &e.from);
        let to_max = widest(|e| &e.to);
        let detail_max = if detail { widest(|e| &e.detail) } else { 0 };
        // O que a linha mais larga precisa depois da coluna `from`.
        let tail_max = events
            .iter()
            .map(|e| e.to.width() + if detail { 1 + e.detail.width() } else { 0 })
            .max()
            .unwrap_or(0);
        let remaining = width.saturating_sub(FIXED_WIDTH + ARROW_WIDTH);
        // `from` inteira se sobra espaço para o resto; senão pelo menos metade.
        let from = from_max.min(remaining.saturating_sub(tail_max).max(remaining / 2));
        let detail_cost = if detail_max > 0 { 1 + detail_max } else { 0 };
        let to = if from + to_max + detail_cost <= remaining {
            to_max
        } else {
            0
        };
        Self {
            width,
            from,
            to,
            detail,
        }
    }

    fn line<'a>(&self, theme: &Theme, ev: &Event, utc: bool) -> Line<'a> {
        let status = ev.source.status();
        let mut spans = vec![
            Span::styled(fmt_clock(ev.ts, utc), theme.muted()),
            Span::raw(" "),
            Span::styled(format!("{} ", status_glyph(status)), theme.status(status)),
            Span::styled(
                pad_to(ev.source.label(), SOURCE_WIDTH),
                theme.status(status),
            ),
            Span::raw(" "),
            Span::styled(pad_to(&ev.action, ACTION_WIDTH), theme.text()),
            Span::raw(" "),
            Span::styled(pad_to(&ev.from, self.from), theme.text()),
            Span::styled(format!(" {} ", ev.arrow()), theme.muted()),
        ];
        let used: usize = spans.iter().map(Span::width).sum();
        let detail_w = if self.detail { ev.detail.width() } else { 0 };
        let reserved = if detail_w > 0 { 1 + detail_w } else { 0 };
        let room = self.width.saturating_sub(used + reserved);
        let to = if self.to > 0 {
            pad_to(&ev.to, self.to.min(room))
        } else {
            fit_to(&ev.to, room)
        };
        let to_w = to.width();
        spans.push(Span::styled(to, theme.text()));
        if detail_w > 0 {
            let gap = self.width.saturating_sub(used + to_w + detail_w).max(1);
            spans.push(Span::raw(" ".repeat(gap)));
            let style = if ev.detail.starts_with('(') {
                theme.muted()
            } else {
                theme.text()
            };
            spans.push(Span::styled(ev.detail.clone(), style));
        }
        Line::from(spans)
    }
}

/// `s` recortada (com `…`) a no máximo `width` colunas de exibição.
pub fn fit_to(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if used + cw > width - 1 {
            break;
        }
        out.push(c);
        used += cw;
    }
    out.push('…');
    out
}

/// `s` recortada e preenchida com espaços até exatamente `width` colunas.
pub fn pad_to(s: &str, width: usize) -> String {
    let mut out = fit_to(s, width);
    let pad = width.saturating_sub(out.width());
    out.extend(std::iter::repeat_n(' ', pad));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use peabiru_discovery::demo;

    #[test]
    fn fit_and_pad_use_display_width() {
        assert_eq!(fit_to("abc", 3), "abc");
        assert_eq!(fit_to("abcdef", 4), "abc…");
        assert_eq!(fit_to("abc", 0), "");
        assert_eq!(fit_to("abc", 1), "…");
        assert_eq!(pad_to("↑/↓", 6), "↑/↓   ");
        assert_eq!(pad_to("abcdef", 4), "abc…");
        assert_eq!(pad_to("", 2), "  ");
    }

    #[test]
    fn window_follows_selection_only_when_paused() {
        assert_eq!(window(7, 6, 6, false), (1, 7));
        assert_eq!(
            window(7, 6, 0, false),
            (1, 7),
            "auto-scroll ignores selection"
        );
        assert_eq!(window(7, 6, 0, true), (0, 6));
        assert_eq!(window(7, 6, 3, true), (0, 6));
        assert_eq!(window(7, 6, 6, true), (1, 7));
        assert_eq!(window(3, 6, 2, true), (0, 3));
        assert_eq!(window(0, 6, 0, false), (0, 0));
        assert_eq!(window(5, 0, 0, false), (0, 0));
    }

    #[test]
    fn demo_timeline_fits_reference_width_without_truncation() {
        let events = demo::events();
        let theme = Theme::default_dark();
        // 156 cols → painel 103 → interior 101 → texto 99.
        let cols = Columns::fit(&events, 99);
        assert!(cols.detail);
        assert_eq!(cols.from, 26, "widest `from` keeps its full width");
        for ev in &events {
            let line = cols.line(&theme, ev, false);
            assert!(line.width() <= 99, "{}", line);
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(!text.contains('…'), "{text}");
            assert!(
                text.contains(&ev.to) && text.ends_with(&ev.detail),
                "{text}"
            );
        }
    }

    #[test]
    fn narrow_width_drops_detail_and_shares_space() {
        let events = demo::events();
        let theme = Theme::default_dark();
        // 100 cols → painel 66 → interior 64 → texto 62.
        let cols = Columns::fit(&events, 62);
        assert!(!cols.detail);
        assert_eq!(cols.to, 0);
        for ev in &events {
            let line = cols.line(&theme, ev, false);
            assert!(line.width() <= 62, "{}", line);
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(!text.contains("(inferred)") && !text.contains("ms"));
        }
    }
}
