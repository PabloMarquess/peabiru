//! Painel LEGEND (ARCHITECTURE §6.2): glifos de nó por status e amostras de
//! linha (observada, inferida, possível) e direção.

use peabiru_core::NodeStatus;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Padding, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::theme::Theme;
use crate::ui::trace_target::{fit, PADDING};
use crate::widgets::{panel_block, status_glyph};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let block = panel_block(theme, "LEGEND", false).padding(Padding::horizontal(PADDING));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let width = usize::from(inner.width);
    let mut lines: Vec<Line> = entries(theme)
        .into_iter()
        .map(|(sample, style, label)| entry(theme, sample, style, label, width))
        .collect();
    lines.truncate(usize::from(inner.height));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Entradas da legenda, na ordem do mockup: `(amostra, estilo da amostra, rótulo)`.
fn entries(theme: &Theme) -> Vec<(&'static str, Style, &'static str)> {
    let node = |s: NodeStatus, label| (status_glyph(s), theme.status(s), label);
    vec![
        node(NodeStatus::Observed, "observed node"),
        node(NodeStatus::Inferred, "inferred node"),
        node(NodeStatus::Correlated, "correlated node"),
        node(NodeStatus::Probe, "probe point"),
        ("──", Style::default().fg(theme.observed), "observed path"),
        ("╌╌", theme.muted(), "inferred path"),
        (
            "┄┄",
            theme.muted().add_modifier(Modifier::DIM),
            "possible path",
        ),
        ("→", theme.text(), "direction"),
    ]
}

/// `<amostra> <rótulo>`: amostra no estilo dado, rótulo em `muted`,
/// truncado para `width`.
fn entry<'a>(
    theme: &Theme,
    sample: &'a str,
    style: Style,
    label: &'a str,
    width: usize,
) -> Line<'a> {
    let sample_width = sample.width();
    if width <= sample_width + 1 {
        return Line::from(Span::styled(fit(sample, width), style));
    }
    let label = fit(label, width - sample_width - 1);
    Line::from(vec![
        Span::styled(sample, style),
        Span::raw(" "),
        Span::styled(label, theme.muted()),
    ])
}
