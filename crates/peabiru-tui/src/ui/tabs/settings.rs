//! Tab SETTINGS (ARCHITECTURE §6.2, "Outros tabs"): tema atual (nome → cor,
//! amostra colorida), configuração/ambiente/flags e o KEYMAP completo.
//!
//! O crate não depende de `peabiru-lua`, então o caminho do `config.lua`
//! não chega até aqui: a linha CONFIG aponta para `--config`.

use std::collections::BTreeSet;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, RunState, MAX_EVENTS};
use crate::theme::Theme;
use crate::ui::help::{key_entry, KEYS};
use crate::widgets::{fmt_duration_ms, kv_line, panel_block};

/// Largura do bloco THEME: 2 colunas de `■■ border_focus  #e5b42e`.
const THEME_W: u16 = 54;
/// Altura da linha THEME/CONFIG (inner 9 = 18 cores em 2 colunas).
const TOP_H: u16 = 11;
/// Largura da coluna de nome numa entrada de tema (`border_focus` = 12).
const NAME_COL: usize = 13;
/// Largura mínima de uma coluna do KEYMAP (`" Esc     close / trace tab"` + folga).
const KEYMAP_COL: usize = 34;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let terminal = frame.area();
    let [top, bottom] =
        Layout::vertical([Constraint::Length(TOP_H), Constraint::Fill(1)]).areas(area);
    let [theme_area, config_area] =
        Layout::horizontal([Constraint::Length(THEME_W), Constraint::Fill(1)]).areas(top);
    render_theme(frame, theme_area, app);
    render_config(frame, config_area, app, terminal);
    render_keymap(frame, bottom, app);
}

fn render_theme(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let block = panel_block(theme, "THEME", false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let names = Theme::names();
    let rows = usize::from(inner.height);
    let cols = names.len().div_ceil(rows.max(1)).max(1);
    let col_w = (usize::from(inner.width) / cols).max(1);
    let lines: Vec<Line> = (0..rows.min(names.len()))
        .map(|row| {
            let mut spans = Vec::new();
            for col in 0..cols {
                let Some(name) = names.get(col * rows + row) else {
                    break;
                };
                let color = theme.get(name).unwrap_or(Color::Reset);
                // `" ■■ "` (4) + nome (13) + `" "` (1) + hex, preenchido até `col_w`.
                let hex = fit(&color_hex(color), col_w.saturating_sub(4 + NAME_COL + 1));
                let pad = col_w.saturating_sub(4 + NAME_COL + 1 + hex.width());
                spans.push(Span::styled(" ■■ ", Style::default().fg(color)));
                spans.push(Span::styled(format!("{name:<NAME_COL$} "), theme.text()));
                spans.push(Span::styled(hex, theme.muted()));
                spans.push(Span::raw(" ".repeat(pad)));
            }
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_config(frame: &mut Frame, area: Rect, app: &App, terminal: Rect) {
    let theme = &app.theme;
    let block = panel_block(theme, "CONFIG", false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let value_w = usize::from(inner.width).saturating_sub(14);
    let env = &app.environment;
    let on_off = |flag: bool| if flag { "on" } else { "off" };
    let flag_style = |flag: bool| {
        if flag {
            Style::default().fg(theme.warn)
        } else {
            theme.muted()
        }
    };
    let rules: BTreeSet<&str> = app.findings.iter().map(|f| f.rule.as_str()).collect();
    let rules_text = if rules.is_empty() {
        "none reported".to_string()
    } else {
        let names: Vec<String> = rules
            .iter()
            .map(|r| {
                let n = app.findings.iter().filter(|f| f.rule == *r).count();
                format!("{r} ({n})")
            })
            .collect();
        names.join(", ")
    };
    let trace_text = match (&app.trace, &app.run) {
        (Some(t), RunState::Done) => format!(
            "#{} completed · {} · {}%",
            t.id,
            fmt_duration_ms(t.summary.duration_ms),
            t.summary.confidence.pct()
        ),
        (Some(t), RunState::Running { .. }) => format!("#{} running", t.id),
        (None, _) if app.last_error.is_some() => "failed".to_string(),
        _ => "idle".to_string(),
    };
    let connected = if env.connected {
        Span::styled(" ● connected", Style::default().fg(theme.ok))
    } else {
        Span::styled(" ● disconnected", Style::default().fg(theme.error))
    };

    let lines = vec![
        kv_line(
            theme,
            "CONFIG",
            Span::styled("(see --config)", theme.muted()),
        ),
        kv_line(theme, "PROVIDER", Span::styled("demo", theme.text())),
        kv_line(
            theme,
            "ENVIRONMENT",
            Span::styled(
                fit(
                    &format!("{} · {} · {}", env.environment, env.cluster, env.user),
                    value_w,
                ),
                theme.title(),
            ),
        ),
        Line::from(vec![
            Span::styled(format!("{:<13}", "ENDPOINT"), theme.muted()),
            Span::styled(fit(&env.endpoint, value_w.saturating_sub(15)), theme.text()),
            connected,
        ]),
        kv_line(
            theme,
            "TERMINAL",
            Span::styled(
                format!("{}x{}", terminal.width, terminal.height),
                theme.text(),
            ),
        ),
        Line::from(vec![
            Span::styled(format!("{:<13}", "FLAGS"), theme.muted()),
            Span::styled("yolo ", theme.text()),
            Span::styled(on_off(app.yolo), flag_style(app.yolo)),
            Span::styled(" · paused ", theme.text()),
            Span::styled(on_off(app.events_paused), flag_style(app.events_paused)),
            Span::styled(" · bg ", theme.text()),
            Span::styled(on_off(theme.paint_bg), theme.muted()),
        ]),
        kv_line(
            theme,
            "RULES",
            Span::styled(fit(&rules_text, value_w), theme.text()),
        ),
        kv_line(
            theme,
            "EVENTS",
            Span::styled(format!("{} / {MAX_EVENTS}", app.events.len()), theme.text()),
        ),
        kv_line(
            theme,
            "TRACE",
            Span::styled(fit(&trace_text, value_w), theme.text()),
        ),
    ];
    let text = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(1),
        ..inner
    };
    frame.render_widget(Paragraph::new(lines), text);
}

fn render_keymap(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let block = panel_block(theme, "KEYMAP", false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    // Tantas colunas quantas forem precisas para caber na altura; com folga,
    // até 3 colunas de ≥ `KEYMAP_COL` para não deixar o painel vazio.
    let width = usize::from(inner.width);
    let height = usize::from(inner.height);
    let cols = KEYS
        .len()
        .div_ceil(height.max(1))
        .max((width / KEYMAP_COL).min(3))
        .max(1);
    let rows = KEYS.len().div_ceil(cols);
    let col_w = (width / cols).max(1);
    let lines: Vec<Line> = (0..rows.min(height))
        .map(|row| {
            let mut spans = Vec::new();
            for col in 0..cols {
                if let Some((key, desc)) = KEYS.get(col * rows + row) {
                    spans.extend(key_entry(theme, key, desc, col_w));
                }
            }
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

/// `Color::Rgb` → `#rrggbb`; demais variantes pelo `Debug` em minúsculas.
pub fn color_hex(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        other => format!("{other:?}").to_lowercase(),
    }
}

/// Trunca `s` a `width` colunas de exibição, terminando em `…` se cortou.
fn fit(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if used + cw > width - 1 {
            break;
        }
        out.push(ch);
        used += cw;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_from_color() {
        assert_eq!(color_hex(Color::Rgb(0x5f, 0xd7, 0x5f)), "#5fd75f");
        assert_eq!(color_hex(Color::Rgb(0, 1, 255)), "#0001ff");
        assert_eq!(color_hex(Color::Reset), "reset");
        assert_eq!(color_hex(Color::Indexed(7)), "indexed(7)");
    }

    #[test]
    fn theme_entries_fit_two_columns() {
        // `" ■■ " + 13 + " " + "#rrggbb"` = 25 colunas por entrada.
        let entry = 4 + NAME_COL + 1 + 7;
        assert!(usize::from(THEME_W) - 2 >= entry * 2);
        assert!(Theme::names().iter().all(|n| n.len() <= NAME_COL));
        assert_eq!(Theme::names().len(), 2 * usize::from(TOP_H - 2));
    }
}
