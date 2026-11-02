//! Popup de detalhes do nó (`d`/`Enter`): 70×24 centralizado com id, kind,
//! provider, endereços, attrs, evidências e arestas (in/out) do nó
//! selecionado (ARCHITECTURE §6.2). `Esc`/`d` fecha.

use peabiru_core::{Edge, Entity, Metrics};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::App;
use crate::theme::Theme;
use crate::widgets::{centered_rect, evidence_glyph, fmt_clock, panel_block_line, status_glyph};

/// Tamanho do popup.
pub const WIDTH: u16 = 70;
pub const HEIGHT: u16 = 24;

/// Largura da coluna de chave nas seções ID/ATTRIBUTES (`"  provider"`).
const KEY_COL: usize = 10;

/// `area` é a tela inteira; o popup se centraliza sozinho.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let rect = centered_rect(WIDTH, HEIGHT, area);
    frame.render_widget(Clear, rect);
    let entity = app.selected_entity();
    let mut title = vec![Span::styled("NODE DETAILS", theme.title())];
    if let Some(e) = entity {
        title.push(Span::styled(format!(" · {}", e.name), theme.text()));
    }
    let block = panel_block_line(theme, Line::from(title), true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.height < 2 || inner.width < 8 {
        return;
    }
    let [body, footer] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);

    match entity {
        Some(e) => {
            // Margem de 1 coluna à esquerda e à direita.
            let width = usize::from(inner.width).saturating_sub(2);
            let mut lines = body_lines(app, e, width);
            let max = usize::from(body.height);
            if lines.len() > max {
                lines.truncate(max.saturating_sub(1));
                lines.push(Line::from(Span::styled("  …", theme.muted())));
            }
            let text = Rect {
                x: body.x + 1,
                width: body.width.saturating_sub(1),
                ..body
            };
            frame.render_widget(Paragraph::new(lines), text);
        }
        None => {
            let hint = Line::from(Span::styled("no node selected", theme.muted()));
            let rect = centered_rect(body.width, 1, body);
            frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), rect);
        }
    }

    let close = Line::from(vec![
        Span::styled("esc", theme.key()),
        Span::styled("/", theme.muted()),
        Span::styled("d", theme.key()),
        Span::styled(" close", theme.muted()),
    ]);
    frame.render_widget(Paragraph::new(close).alignment(Alignment::Center), footer);
}

/// Todas as linhas do corpo (sem limite de altura), já truncadas a `width`.
fn body_lines(app: &App, e: &Entity, width: usize) -> Vec<Line<'static>> {
    let theme = &app.theme;
    let mut lines = vec![
        section(theme, "ID"),
        kv(theme, "id", &e.id, theme.text(), width),
        kv(theme, "kind", e.kind.label(), theme.text(), width),
        kv(theme, "provider", e.provider.label(), theme.text(), width),
    ];

    let status = e.status();
    lines.push(fit_line(
        vec![
            (format!("  {} ", status_glyph(status)), theme.status(status)),
            (
                status.label().to_string(),
                theme.status(status).add_modifier(Modifier::BOLD),
            ),
            (format!("  confidence {}", e.confidence()), theme.muted()),
        ],
        width,
    ));

    lines.push(section(
        theme,
        &format!("ADDRESSES ({})", e.addresses.len()),
    ));
    if e.addresses.is_empty() {
        lines.push(Line::from(Span::styled("  none", theme.muted())));
    }
    for a in &e.addresses {
        let scope = match a.ip {
            Some(_) if a.is_private() => "  private",
            Some(_) => "  public",
            None => "  hostname",
        };
        lines.push(fit_line(
            vec![
                (format!("  {}", a.display()), theme.text()),
                (scope.to_string(), theme.muted()),
            ],
            width,
        ));
    }

    lines.push(section(theme, &format!("ATTRIBUTES ({})", e.attrs.len())));
    if e.attrs.is_empty() {
        lines.push(Line::from(Span::styled("  none", theme.muted())));
    }
    for (k, v) in &e.attrs {
        lines.push(kv(theme, k, v, theme.text(), width));
    }

    lines.push(section(theme, &format!("EVIDENCE ({})", e.evidence.len())));
    if e.evidence.is_empty() {
        lines.push(Line::from(Span::styled("  none", theme.muted())));
    }
    for ev in &e.evidence {
        let glyph_style = if ev.kind.is_observational() {
            Style::default().fg(theme.observed)
        } else {
            theme.muted()
        };
        let pct = ev.confidence.pct();
        // `LOG_CONFIRMED` (13) alinha as fontes numa coluna.
        let head = vec![
            (format!("  {} ", evidence_glyph(ev.kind)), glyph_style),
            (format!("{:<13}  ", ev.kind.label()), theme.text()),
        ];
        let tail = vec![
            (format!("  {}", ev.provider.label()), theme.muted()),
            (
                format!("  {pct:>3}%"),
                Style::default().fg(theme.score_color(pct)),
            ),
            (
                format!("  {}", fmt_clock(ev.timestamp, app.fixed_clock.is_some())),
                theme.muted(),
            ),
        ];
        lines.push(head_body_tail(
            head,
            (ev.source.clone(), theme.text()),
            tail,
            width,
        ));
    }

    let in_edges = app.graph.in_edges(&e.id);
    lines.push(section(theme, &format!("EDGES IN ({})", in_edges.len())));
    if in_edges.is_empty() {
        lines.push(Line::from(Span::styled("  none", theme.muted())));
    }
    for (_, edge) in &in_edges {
        lines.push(edge_line(app, edge, &edge.from, "←", width));
    }

    let out_edges = app.graph.out_edges(&e.id);
    lines.push(section(theme, &format!("EDGES OUT ({})", out_edges.len())));
    if out_edges.is_empty() {
        lines.push(Line::from(Span::styled("  none", theme.muted())));
    }
    for (_, edge) in &out_edges {
        lines.push(edge_line(app, edge, &edge.to, "→", width));
    }

    lines
}

/// `  → nome  relação  STATUS  92%  rtt 1.217ms retx 3 drops 1 rst 2`.
fn edge_line(app: &App, edge: &Edge, other: &str, arrow: &str, width: usize) -> Line<'static> {
    let theme = &app.theme;
    let name = app
        .graph
        .entity(other)
        .map_or_else(|| other.to_string(), |e| e.name.clone());
    let status = edge.status();
    let pct = edge.confidence().pct();
    let mut spans = vec![
        (format!("  {arrow} "), theme.status(status)),
        (name, theme.text()),
        (format!("  {}", edge.relation.label()), theme.muted()),
        (
            format!("  {}", status.label()),
            theme.status(status).add_modifier(Modifier::BOLD),
        ),
        (
            format!("  {pct:>3}%"),
            Style::default().fg(theme.score_color(pct)),
        ),
    ];
    let metrics = fmt_metrics(&edge.metrics);
    if !metrics.is_empty() {
        spans.push((format!("  {metrics}"), theme.muted()));
    }
    fit_line(spans, width)
}

/// Só os campos não nulos: `rtt 1.217ms retx 3 drops 1 rst 2`.
fn fmt_metrics(m: &Metrics) -> String {
    let mut parts = Vec::new();
    if let Some(rtt) = m.rtt_ms {
        parts.push(format!("rtt {rtt:.3}ms"));
    }
    if m.retransmissions > 0 {
        parts.push(format!("retx {}", m.retransmissions));
    }
    if m.drops > 0 {
        parts.push(format!("drops {}", m.drops));
    }
    if m.rst > 0 {
        parts.push(format!("rst {}", m.rst));
    }
    parts.join(" ")
}

fn section(theme: &Theme, title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        theme.title().add_modifier(Modifier::BOLD),
    ))
}

/// `  chave     valor` — chave em `muted` até `KEY_COL`, valor truncado.
fn kv(theme: &Theme, key: &str, value: &str, style: Style, width: usize) -> Line<'static> {
    fit_line(
        vec![
            (format!("  {key:<KEY_COL$} "), theme.muted()),
            (value.to_string(), style),
        ],
        width,
    )
}

/// Linha `head + corpo + tail` em que só o corpo é truncado, preservando o
/// `tail` (confiança, hora). Se nem `head + tail` cabem, trunca tudo.
fn head_body_tail(
    head: Vec<(String, Style)>,
    body: (String, Style),
    tail: Vec<(String, Style)>,
    width: usize,
) -> Line<'static> {
    let fixed: usize = head.iter().chain(tail.iter()).map(|(s, _)| s.width()).sum();
    if fixed + 4 > width {
        let mut all = head;
        all.push(body);
        all.extend(tail);
        return fit_line(all, width);
    }
    let body_w = width - fixed;
    let mut spans: Vec<Span<'static>> = head
        .into_iter()
        .map(|(s, st)| Span::styled(s, st))
        .collect();
    spans.push(Span::styled(fit(&body.0, body_w), body.1));
    spans.extend(tail.into_iter().map(|(s, st)| Span::styled(s, st)));
    Line::from(spans)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_truncates_with_ellipsis() {
        assert_eq!(fit("abc", 5), "abc");
        assert_eq!(fit("abcdef", 4), "abc…");
        assert_eq!(fit("abcdef", 1), "…");
        assert_eq!(fit("abcdef", 0), "");
        assert_eq!(fit("日本語", 3), "日…");
    }

    #[test]
    fn fit_line_drops_trailing_spans() {
        let s = Style::default();
        let line = fit_line(
            vec![("ab".into(), s), ("cdef".into(), s), ("gh".into(), s)],
            5,
        );
        let text: String = line.spans.iter().map(|sp| sp.content.as_ref()).collect();
        assert_eq!(text, "abcd…");
    }

    #[test]
    fn metrics_only_nonzero() {
        assert_eq!(fmt_metrics(&Metrics::default()), "");
        let m = Metrics {
            rtt_ms: Some(1.217),
            retransmissions: 0,
            drops: 2,
            rst: 0,
        };
        assert_eq!(fmt_metrics(&m), "rtt 1.217ms drops 2");
    }
}
