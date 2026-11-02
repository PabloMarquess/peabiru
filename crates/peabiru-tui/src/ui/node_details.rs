//! Painel NODE DETAILS (ARCHITECTURE §6.2): cabeçalho `◇ nome  [STATUS]`,
//! endereços e atributos, STATUS / SOURCE / LAST SEEN / CONFIDENCE, lista de
//! evidências e rodapé de teclas pinado na última linha. Toda linha é
//! recortada à largura do painel (`…`), nunca transborda.

use peabiru_core::{Entity, Timestamp};
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Panel, TICK_MS};
use crate::theme::Theme;
use crate::widgets;

/// Blocos da barra de CONFIDENCE.
const BAR_BLOCKS: usize = 12;
/// Attrs que já aparecem noutra linha (TYPE, STATUS) ou são internos (probe).
const HIDDEN_ATTRS: &[&str] = &["type", "probe", "reachable"];
/// Marca de truncamento (1 coluna).
pub(crate) const ELLIPSIS: &str = "…";

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let block = widgets::panel_block(theme, "NODE DETAILS", app.focus == Panel::NodeDetails);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    // Uma coluna de respiro entre a borda e o texto, como no mockup.
    let content = inner.inner(Margin::new(1, 0));
    if content.width == 0 || content.height == 0 {
        return;
    }
    let width = usize::from(content.width);

    // Rodapé pinado na última linha (quando há espaço para corpo + rodapé).
    let body_height = if content.height >= 2 {
        content.height - 1
    } else {
        content.height
    };
    let body_rows = usize::from(body_height);

    let lines = match app.selected_entity() {
        Some(entity) => body_lines(theme, entity, width, body_rows, app.now()),
        None => vec![Line::from(Span::styled(
            "select a node in the graph",
            theme.muted(),
        ))],
    };
    let lines: Vec<Line> = lines
        .into_iter()
        .take(body_rows)
        .map(|l| fit_line(l, width))
        .collect();
    let body = Rect {
        height: body_height,
        ..content
    };
    frame.render_widget(Paragraph::new(lines), body);

    if content.height >= 2 {
        let footer = Rect::new(content.x, content.bottom() - 1, content.width, 1);
        frame.render_widget(
            Paragraph::new(fit_line(footer_line(theme, width), width)),
            footer,
        );
    }
}

/// Corpo do painel para `entity`, com no máximo `rows` linhas (a lista de
/// evidências é a parte elástica; antes de encolhê-la, a linha em branco
/// abaixo do cabeçalho é sacrificada). `now` é o "agora" de LAST SEEN.
fn body_lines(
    theme: &Theme,
    entity: &Entity,
    width: usize,
    rows: usize,
    now: Timestamp,
) -> Vec<Line<'static>> {
    let status = entity.status();
    let mut lines = vec![
        Line::from(vec![
            widgets::status_span(theme, status, widgets::status_glyph(status)),
            Span::raw(" "),
            Span::styled(
                entity.name.clone(),
                theme.status(status).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            widgets::badge(theme, status),
        ]),
        Line::default(),
    ];

    if let Some(host) = entity.addresses.iter().find_map(|a| a.hostname.as_deref()) {
        lines.push(kv(theme, "ADDRESS", host));
    }
    if let Some(addr) = entity.addresses.iter().find(|a| a.ip.is_some()) {
        lines.push(kv(theme, "IP:PORT", addr.display()));
    }
    let kind = entity
        .attrs
        .get("type")
        .map_or(entity.kind.label(), String::as_str);
    lines.push(kv(theme, "TYPE", kind));
    for (key, value) in &entity.attrs {
        if !HIDDEN_ATTRS.contains(&key.as_str()) {
            lines.push(kv(theme, &key.to_uppercase(), value.as_str()));
        }
    }

    lines.push(separator(theme, width));
    lines.push(reachability(theme, entity));
    let sources = entity.sources().len();
    let noun = if sources == 1 { "sensor" } else { "sensors" };
    lines.push(kv(theme, "SOURCE", format!("{sources} {noun}")));
    lines.push(last_seen(theme, entity, now));
    lines.push(confidence(theme, entity.confidence().pct()));
    lines.push(separator(theme, width));

    // Sem espaço para `EVIDENCE (n)` + todas as evidências: a linha em
    // branco do cabeçalho vale menos que uma evidência visível.
    if lines.len() + 1 + entity.evidence.len() > rows {
        lines.remove(1);
    }
    let budget = rows.saturating_sub(lines.len());
    lines.extend(evidence_lines(theme, entity, budget));
    lines
}

/// `KEY          value` (chave em `muted`, valor em `text`).
fn kv(theme: &Theme, key: &str, value: impl Into<String>) -> Line<'static> {
    widgets::kv_line(theme, key, Span::styled(value.into(), theme.text()))
}

/// Linha `─────` na cor da borda, com a largura do conteúdo.
fn separator(theme: &Theme, width: usize) -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(width),
        Style::default().fg(theme.border),
    ))
}

/// STATUS: `reachable` (ok) / `unreachable` (error) / `unknown` (muted),
/// a partir de `attrs["reachable"]`.
fn reachability(theme: &Theme, entity: &Entity) -> Line<'static> {
    let (text, style) = match entity.attrs.get("reachable").map(String::as_str) {
        Some("true") => ("reachable", Style::default().fg(theme.ok)),
        Some("false") => ("unreachable", Style::default().fg(theme.error)),
        _ => ("unknown", theme.muted()),
    };
    widgets::kv_line(theme, "STATUS", Span::styled(text, style))
}

/// LAST SEEN: idade da evidência mais recente relativa a `now`, na
/// resolução do tick (`never` sem evidência).
fn last_seen(theme: &Theme, entity: &Entity, now: Timestamp) -> Line<'static> {
    let newest = entity.evidence.iter().map(|e| e.timestamp).max();
    let value = match newest {
        Some(ts) => Span::styled(widgets::fmt_ago(age_ms(ts, now)), theme.text()),
        None => Span::styled("never", theme.muted()),
    };
    widgets::kv_line(theme, "LAST SEEN", value)
}

/// Ms entre `seen` e `now`, arredondados para baixo ao múltiplo de `TICK_MS`
/// (a UI avança em ticks de 100 ms; assim o valor é estável dentro de um
/// tick). `seen` no futuro conta como 0.
fn age_ms(seen: Timestamp, now: Timestamp) -> u64 {
    let ms = u64::try_from((now - seen).num_milliseconds()).unwrap_or(0);
    ms - ms % TICK_MS
}

/// CONFIDENCE: `92%` + barra de `BAR_BLOCKS` blocos.
fn confidence(theme: &Theme, pct: u8) -> Line<'static> {
    let mut line = widgets::kv_line(
        theme,
        "CONFIDENCE",
        Span::styled(format!("{pct}%  "), theme.text()),
    );
    line.spans
        .extend(widgets::confidence_bar(theme, pct, BAR_BLOCKS).spans);
    line
}

/// `EVIDENCE (n)` + uma linha `● <source>` por evidência (glifo `●` em
/// `observed` se observacional, `○` em `muted` senão). Se não couberem em
/// `budget` linhas (cabeçalho incluído), a última vira `… +N more`.
fn evidence_lines(theme: &Theme, entity: &Entity, budget: usize) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    if budget == 0 {
        return out;
    }
    let total = entity.evidence.len();
    out.push(Line::from(Span::styled(
        format!("EVIDENCE ({total})"),
        theme.muted(),
    )));
    let rows = budget - 1;
    let shown = if total <= rows {
        total
    } else {
        rows.saturating_sub(1)
    };
    for ev in entity.evidence.iter().take(shown) {
        let glyph_style = if ev.kind.is_observational() {
            Style::default().fg(theme.observed)
        } else {
            theme.muted()
        };
        out.push(Line::from(vec![
            Span::styled(widgets::evidence_glyph(ev.kind), glyph_style),
            Span::raw(" "),
            Span::styled(ev.source.clone(), theme.text()),
        ]));
    }
    if shown < total && rows > 0 {
        out.push(Line::from(Span::styled(
            format!("{ELLIPSIS} +{} more", total - shown),
            theme.muted(),
        )));
    }
    out
}

/// `[d] view details    [y] yolo mode`; em painéis estreitos a versão
/// compacta `[d] details  [y] yolo`.
fn footer_line(theme: &Theme, width: usize) -> Line<'static> {
    let hints = |details: &str, gap: &str, yolo: &str| {
        Line::from(vec![
            Span::styled("[d]", theme.key()),
            Span::styled(details.to_string(), theme.text()),
            Span::raw(gap.to_string()),
            Span::styled("[y]", theme.key()),
            Span::styled(yolo.to_string(), theme.text()),
        ])
    };
    let full = hints(" view details", "    ", " yolo mode");
    if line_width(&full) <= width {
        full
    } else {
        hints(" details", "  ", " yolo")
    }
}

// --- helpers de texto (compartilhados com `correlations`) ----------------

/// Recorta `line` para caber em `width` colunas (largura unicode), com `…`
/// no fim quando algo foi cortado. Preserva o estilo de cada span e o da
/// linha.
pub(crate) fn fit_line(line: Line<'_>, width: usize) -> Line<'_> {
    if line_width(&line) <= width {
        return line;
    }
    let style = line.style;
    let alignment = line.alignment;
    let mut out: Vec<Span> = Vec::new();
    if width > 0 {
        let keep = width - ELLIPSIS.width();
        let mut used = 0;
        let mut last_style = style;
        for span in line.spans {
            let w = span.content.width();
            last_style = span.style;
            if used + w <= keep {
                used += w;
                out.push(span);
                continue;
            }
            let mut partial = String::new();
            for ch in span.content.chars() {
                let cw = ch.width().unwrap_or(0);
                if used + cw > keep {
                    break;
                }
                used += cw;
                partial.push(ch);
            }
            if !partial.is_empty() {
                out.push(Span::styled(partial, span.style));
            }
            break;
        }
        out.push(Span::styled(ELLIPSIS, last_style));
    }
    Line {
        spans: out,
        style,
        alignment,
    }
}

/// Largura (unicode) de `line`.
pub(crate) fn line_width(line: &Line<'_>) -> usize {
    line.spans.iter().map(|s| s.content.width()).sum()
}

/// `s` recortado a `width` colunas, com `…` no fim se foi cortado.
pub(crate) fn truncate_str(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let keep = width - ELLIPSIS.width();
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if used + cw > keep {
            break;
        }
        used += cw;
        out.push(ch);
    }
    out.push_str(ELLIPSIS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn truncate_str_adds_ellipsis() {
        assert_eq!(truncate_str("aws-alb", 14), "aws-alb");
        assert_eq!(truncate_str("istio-ingressgateway", 14), "istio-ingress…");
        assert_eq!(truncate_str("abc", 1), "…");
        assert_eq!(truncate_str("abc", 0), "");
        assert_eq!(truncate_str("日本語", 3), "日…");
    }

    #[test]
    fn fit_line_keeps_short_lines_and_cuts_long_ones() {
        let short = Line::from(vec![Span::raw("ab"), Span::raw("cd")]);
        assert_eq!(text(&fit_line(short, 4)), "abcd");
        let long = Line::from(vec![
            Span::raw("ADDRESS  "),
            Span::raw("alb2c3.elb.amazonaws.com"),
        ]);
        let cut = fit_line(long, 20);
        assert_eq!(text(&cut), "ADDRESS  alb2c3.elb…");
        assert_eq!(cut.spans.len(), 3);
        let empty = fit_line(Line::from("hello"), 0);
        assert_eq!(text(&empty), "");
        let only_ellipsis = fit_line(Line::from("hello"), 1);
        assert_eq!(text(&only_ellipsis), "…");
    }

    #[test]
    fn evidence_list_collapses_to_more() {
        use peabiru_core::{Confidence, EntityKind, Evidence, EvidenceKind, Provider};
        let theme = Theme::default_dark();
        let mut entity = Entity::new("x://1", "x", EntityKind::Host, Provider::Demo);
        for i in 0..5 {
            entity = entity.with_evidence(Evidence::new(
                EvidenceKind::Observed,
                &format!("src-{i}"),
                Provider::Demo,
                Confidence::new(90),
            ));
        }
        let all = evidence_lines(&theme, &entity, 6);
        assert_eq!(all.len(), 6);
        assert_eq!(text(&all[0]), "EVIDENCE (5)");
        assert_eq!(text(&all[5]), "● src-4");
        let some = evidence_lines(&theme, &entity, 4);
        assert_eq!(some.len(), 4);
        assert_eq!(text(&some[3]), "… +3 more");
        let header_only = evidence_lines(&theme, &entity, 1);
        assert_eq!(header_only.len(), 1);
        assert!(evidence_lines(&theme, &entity, 0).is_empty());
    }

    #[test]
    fn reachability_states() {
        use chrono::Utc;
        use peabiru_core::{EntityKind, Provider};
        let theme = Theme::default_dark();
        let base = Entity::new("x://1", "x", EntityKind::Host, Provider::Demo);
        assert_eq!(
            text(&reachability(&theme, &base)).trim_end(),
            "STATUS       unknown"
        );
        let up = base.clone().with_attr("reachable", "true");
        assert!(text(&reachability(&theme, &up)).ends_with("reachable"));
        let down = base.with_attr("reachable", "false");
        assert!(text(&reachability(&theme, &down)).ends_with("unreachable"));
        assert_eq!(
            text(&last_seen(&theme, &up, Utc::now())).trim_end(),
            "LAST SEEN    never"
        );
    }

    #[test]
    fn age_is_quantized_to_the_tick_and_never_negative() {
        use chrono::{TimeDelta, Utc};
        let now = Utc::now();
        assert_eq!(age_ms(now, now), 0);
        assert_eq!(age_ms(now + TimeDelta::seconds(5), now), 0, "future");
        assert_eq!(age_ms(now - TimeDelta::milliseconds(2843), now), 2800);
        assert_eq!(age_ms(now - TimeDelta::milliseconds(99), now), 0);
        assert_eq!(age_ms(now - TimeDelta::milliseconds(100), now), 100);
    }

    #[test]
    fn footer_compacts_when_narrow() {
        let theme = Theme::default_dark();
        assert_eq!(
            text(&footer_line(&theme, 40)),
            "[d] view details    [y] yolo mode"
        );
        assert_eq!(text(&footer_line(&theme, 24)), "[d] details  [y] yolo");
    }
}
