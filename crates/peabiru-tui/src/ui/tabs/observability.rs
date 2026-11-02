//! Tab OBSERVABILITY (ARCHITECTURE §6.2, "Outros tabs"): FINDINGS das regras
//! Lua (`app.findings`), PATH METRICS do trace e contadores de EVENT SOURCES.

use peabiru_core::{EventSource, Finding, NodeStatus, Severity};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::App;
use crate::widgets::{
    centered_rect, confidence_bar, fmt_duration_ms, kv_line, panel_block, panel_block_with_right,
    status_glyph,
};

/// Todas as fontes, na ordem de exibição.
const SOURCES: [EventSource; 6] = [
    EventSource::Ebpf,
    EventSource::FlowLog,
    EventSource::Correlation,
    EventSource::Inference,
    EventSource::Probe,
    EventSource::Discovery,
];

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);
    let [metrics, sources] =
        Layout::vertical([Constraint::Length(9), Constraint::Fill(1)]).areas(right);
    render_findings(frame, left, app);
    render_metrics(frame, metrics, app);
    render_sources(frame, sources, app);
}

fn render_findings(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let block = panel_block(theme, &format!("FINDINGS ({})", app.findings.len()), true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    if app.findings.is_empty() {
        let line = if app.trace.is_none() {
            Line::from(Span::styled("run a trace first", theme.muted()))
        } else {
            Line::from(vec![
                Span::styled("no findings", Style::default().fg(theme.ok)),
                Span::styled(" (rules: none triggered)", theme.muted()),
            ])
        };
        let rect = centered_rect(inner.width, 1, inner);
        frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), rect);
        return;
    }

    let width = usize::from(inner.width).saturating_sub(2);
    let max = usize::from(inner.height);
    let mut lines: Vec<Line> = Vec::new();
    for f in &app.findings {
        if lines.len() + 1 == max && app.findings.len() > max {
            let more = app.findings.len() - lines.len();
            lines.push(Line::from(Span::styled(
                format!("… +{more} more"),
                theme.muted(),
            )));
            break;
        }
        lines.push(finding_line(app, f, width));
    }
    let text = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(1),
        ..inner
    };
    frame.render_widget(Paragraph::new(lines), text);
}

/// `[WARNING] mensagem  default.lua @ nó`.
fn finding_line(app: &App, f: &Finding, width: usize) -> Line<'static> {
    let theme = &app.theme;
    let color = severity_color(app, f.severity);
    let mut parts = vec![
        (
            format!("[{}]", f.severity.label().to_uppercase()),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        (format!(" {}", f.message), theme.text()),
        (format!("  {}", f.rule), theme.muted()),
    ];
    if let Some(subject) = &f.subject {
        let name = app
            .graph
            .entity(subject)
            .map_or_else(|| subject.clone(), |e| e.name.clone());
        parts.push((format!(" @ {name}"), theme.muted()));
    }
    fit_line(parts, width)
}

/// info → `title`, warning → `warn`, critical → `error`.
fn severity_color(app: &App, s: Severity) -> Color {
    match s {
        Severity::Info => app.theme.title,
        Severity::Warning => app.theme.warn,
        Severity::Critical => app.theme.error,
    }
}

fn render_metrics(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let block = panel_block(theme, "PATH METRICS", false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let Some(trace) = &app.trace else {
        let hint = Line::from(Span::styled("run a trace first", theme.muted()));
        let rect = centered_rect(inner.width, 1, inner);
        frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), rect);
        return;
    };

    let s = &trace.summary;
    let counter = |n: u32, bad: Color| -> Span<'static> {
        let style = if n > 0 {
            Style::default().fg(bad).add_modifier(Modifier::BOLD)
        } else {
            theme.text()
        };
        Span::styled(n.to_string(), style)
    };
    let rtt = trace
        .max_rtt_ms()
        .map_or_else(|| "n/a".to_string(), |ms| format!("{ms:.3} ms"));
    let pct = s.confidence.pct();
    let mut confidence = vec![
        Span::styled(format!("{:<13}", "CONFIDENCE"), theme.muted()),
        Span::styled(
            format!("{pct}%  "),
            Style::default()
                .fg(theme.score_color(pct))
                .add_modifier(Modifier::BOLD),
        ),
    ];
    confidence.extend(confidence_bar(theme, pct, 12).spans);
    let lines = vec![
        Line::from(vec![
            Span::styled(format!("{:<13}", "HOPS"), theme.muted()),
            Span::styled(s.hops.to_string(), theme.text()),
            Span::styled("  ", theme.muted()),
            Span::styled(
                format!("{} {}", status_glyph(NodeStatus::Observed), s.observed),
                Style::default().fg(theme.observed),
            ),
            Span::styled(
                format!("  {} {}", status_glyph(NodeStatus::Inferred), s.inferred),
                theme.muted(),
            ),
        ]),
        kv_line(theme, "MAX RTT", Span::styled(rtt, theme.text())),
        kv_line(
            theme,
            "RETRANSMITS",
            counter(trace.total_retransmissions(), theme.warn),
        ),
        kv_line(theme, "DROPS", counter(trace.total_drops(), theme.error)),
        kv_line(theme, "RST", counter(trace.total_rst(), theme.warn)),
        Line::from(confidence),
        kv_line(
            theme,
            "DURATION",
            Span::styled(fmt_duration_ms(s.duration_ms), theme.text()),
        ),
    ];
    let text = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(1),
        ..inner
    };
    frame.render_widget(Paragraph::new(lines), text);
}

fn render_sources(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let right = if app.events_paused {
        Line::from(Span::styled(" paused ", Style::default().fg(theme.warn)))
    } else {
        Line::default()
    };
    let block = panel_block_with_right(
        theme,
        &format!("EVENT SOURCES ({})", app.events.len()),
        right,
        false,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let counts: Vec<(EventSource, usize)> = SOURCES
        .iter()
        .map(|s| (*s, app.events.iter().filter(|e| e.source == *s).count()))
        .collect();
    let max = counts.iter().map(|(_, n)| *n).max().unwrap_or(0);
    // `" ● correlation  123 "` = 20 colunas antes da barra.
    let bar_w = usize::from(inner.width).saturating_sub(21);
    let mut lines: Vec<Line> = counts
        .iter()
        .map(|(source, n)| {
            let status = source.status();
            let filled = if max == 0 {
                0
            } else {
                (n * bar_w).div_ceil(max)
            };
            Line::from(vec![
                Span::styled(format!(" {} ", status_glyph(status)), theme.status(status)),
                Span::styled(format!("{:<12}", source.label()), theme.text()),
                Span::styled(format!("{n:>4} "), theme.text()),
                Span::styled("■".repeat(filled), theme.status(status)),
            ])
        })
        .collect();
    let total: usize = counts.iter().map(|(_, n)| *n).sum();
    lines.push(Line::from(vec![
        Span::styled(format!("   {:<12}", "total"), theme.muted()),
        Span::styled(format!("{total:>4}"), theme.muted()),
    ]));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Monta uma linha a partir de `(texto, estilo)`, truncando com `…` o
/// primeiro span que ultrapassar `width` e descartando os seguintes.
fn fit_line(parts: Vec<(String, Style)>, width: usize) -> Line<'static> {
    let mut spans = Vec::new();
    let mut used = 0usize;
    for (text, style) in parts {
        let w = text.width();
        if used + w <= width {
            used += w;
            spans.push(Span::styled(text, style));
            continue;
        }
        let rest = width.saturating_sub(used);
        if rest > 0 {
            spans.push(Span::styled(fit(&text, rest), style));
        }
        break;
    }
    Line::from(spans)
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
