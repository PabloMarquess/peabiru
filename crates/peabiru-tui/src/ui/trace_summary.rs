//! Painel TRACE SUMMARY (ARCHITECTURE §6.2): STATUS, DURATION, HOPS,
//! OBSERVED/INFERRED com glifo e percentual, CORRELATIONS, CONFIDENCE + barra.

use peabiru_core::NodeStatus;
use peabiru_trace::TraceStatus;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Padding, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;
use crate::ui::trace_target::{fit, PADDING};
use crate::widgets::{
    confidence_bar, fmt_duration_ms, kv_line, panel_block, status_glyph, KEY_WIDTH,
};

/// Blocos da barra de confiança.
pub const BAR_BLOCKS: usize = 12;

/// Valor exibido quando ainda não há trace.
pub const NONE: &str = "-";

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let block = panel_block(theme, "TRACE SUMMARY", false).padding(Padding::horizontal(PADDING));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let width = usize::from(inner.width);
    let value_width = width.saturating_sub(KEY_WIDTH);
    let summary = app.trace.as_ref().map(|t| &t.summary);
    let status = app.trace_status();

    let mut lines = vec![kv_line(
        theme,
        "STATUS",
        status_span(theme, status, value_width),
    )];
    if status == TraceStatus::Failed {
        let err = app.last_error.as_deref().unwrap_or("trace failed");
        lines.push(Line::from(Span::styled(
            fit(err, width),
            Style::default().fg(theme.error),
        )));
    }
    lines.push(kv_line(
        theme,
        "DURATION",
        plain(
            theme,
            summary.map(|s| fmt_duration_ms(s.duration_ms)),
            value_width,
        ),
    ));
    lines.push(kv_line(
        theme,
        "HOPS",
        plain(theme, summary.map(|s| s.hops.to_string()), value_width),
    ));
    lines.push(counted(
        theme,
        "OBSERVED",
        NodeStatus::Observed,
        summary.map(|s| (s.observed, s.hops)),
        value_width,
    ));
    lines.push(counted(
        theme,
        "INFERRED",
        NodeStatus::Inferred,
        summary.map(|s| (s.inferred, s.hops)),
        value_width,
    ));
    lines.push(kv_line(
        theme,
        "CORRELATIONS",
        plain(
            theme,
            summary.map(|s| s.correlations.to_string()),
            value_width,
        ),
    ));
    let pct = summary.map(|s| s.confidence.pct());
    let confidence = match pct {
        Some(p) => Span::styled(
            fit(&format!("{p}%"), value_width),
            Style::default().fg(theme.score_color(p)),
        ),
        None => Span::styled(fit(NONE, value_width), theme.muted()),
    };
    lines.push(kv_line(theme, "CONFIDENCE", confidence));
    if value_width >= BAR_BLOCKS {
        let mut bar = confidence_bar(theme, pct.unwrap_or(0), BAR_BLOCKS);
        bar.spans.insert(0, Span::raw(" ".repeat(KEY_WIDTH)));
        lines.push(bar);
    }

    lines.truncate(usize::from(inner.height));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// STATUS colorido por estado: Completed `ok`, Running `warn`, Idle `muted`,
/// Failed `error`.
fn status_span<'a>(theme: &Theme, status: TraceStatus, width: usize) -> Span<'a> {
    let (label, color) = match status {
        TraceStatus::Idle => ("Idle", theme.muted),
        TraceStatus::Running => ("Running", theme.warn),
        TraceStatus::Completed => ("Completed", theme.ok),
        TraceStatus::Failed => ("Failed", theme.error),
    };
    Span::styled(fit(label, width), Style::default().fg(color))
}

/// Valor em `text`, ou [`NONE`] em `muted` quando não há trace.
fn plain<'a>(theme: &Theme, value: Option<String>, width: usize) -> Span<'a> {
    match value {
        Some(v) => Span::styled(fit(&v, width), theme.text()),
        None => Span::styled(fit(NONE, width), theme.muted()),
    }
}

/// `● 7 (50%)`: glifo na cor do status, contagem e percentual dos hops
/// (`count = (parte, total)`); [`NONE`] sem trace.
fn counted<'a>(
    theme: &Theme,
    key: &str,
    status: NodeStatus,
    count: Option<(usize, usize)>,
    width: usize,
) -> Line<'a> {
    let mut line = kv_line(theme, key, Span::raw(""));
    line.spans.pop();
    if width == 0 {
        return line;
    }
    line.spans
        .push(Span::styled(status_glyph(status), theme.status(status)));
    if width < 3 {
        return line;
    }
    line.spans.push(Span::raw(" "));
    let rest = width - 2;
    match count {
        Some((n, total)) => {
            let text = format!("{n} ({}%)", percent(n, total));
            line.spans
                .push(Span::styled(fit(&text, rest), theme.text()));
        }
        None => line
            .spans
            .push(Span::styled(fit(NONE, rest), theme.muted())),
    }
    line
}

/// `part` de `total` em % inteiro arredondado (0 se `total == 0`).
pub fn percent(part: usize, total: usize) -> usize {
    if total == 0 {
        0
    } else {
        (part * 100 + total / 2) / total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_rounds() {
        assert_eq!(percent(0, 0), 0);
        assert_eq!(percent(7, 14), 50);
        assert_eq!(percent(5, 5), 100);
        assert_eq!(percent(1, 3), 33);
        assert_eq!(percent(2, 3), 67);
    }
}
