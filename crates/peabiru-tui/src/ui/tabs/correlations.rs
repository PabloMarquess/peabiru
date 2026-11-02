//! Tab CORRELATIONS (ARCHITECTURE §6.2, "Outros tabs"): tabela completa das
//! correlações do trace (cursor = `app.selected_correlation`) e, à direita,
//! o detalhe do par selecionado.

use peabiru_trace::Correlation;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::App;
use crate::widgets::{
    centered_rect, confidence_bar, kv_line, panel_block, status_glyph, KEY_WIDTH,
};

const NAME_MAX: usize = 26;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);
    let correlations: &[Correlation] = match &app.trace {
        Some(t) => &t.correlations,
        None => &[],
    };
    let selected = (!correlations.is_empty()).then(|| {
        let idx = app.selected_correlation.min(correlations.len() - 1);
        (idx, &correlations[idx])
    });
    render_table(frame, left, app, correlations, selected.map(|(i, _)| i));
    render_detail(frame, right, app, selected.map(|(_, c)| c));
}

fn render_table(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    correlations: &[Correlation],
    selected: Option<usize>,
) {
    let theme = &app.theme;
    let block = panel_block(
        theme,
        &format!("CORRELATIONS ({})", correlations.len()),
        true,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    // Uma coluna de margem à esquerda (a tabela não cola na borda).
    let inner = Rect {
        x: inner.x.saturating_add(1),
        width: inner.width.saturating_sub(1),
        ..inner
    };
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    if correlations.is_empty() {
        let hint = Line::from(vec![
            Span::styled("run a trace first", theme.muted()),
            Span::styled("  (", theme.muted()),
            Span::styled("r", theme.key()),
            Span::styled(")", theme.muted()),
        ]);
        let rect = centered_rect(inner.width, 1, inner);
        frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), rect);
        return;
    }

    let name_w = col_width(
        correlations
            .iter()
            .flat_map(|c| [c.a_name.width(), c.b_name.width()]),
        4,
        NAME_MAX,
    );
    let fixed = name_w + 3 + name_w + 5 + 4;
    let basis_w = usize::from(inner.width.saturating_sub(fixed));

    let rows = correlations.iter().map(|c| {
        let pct = c.score.pct();
        let basis = if c.basis.is_empty() {
            "—"
        } else {
            c.basis.as_str()
        };
        Row::new(vec![
            Cell::from(Span::styled(
                fit(&c.a_name, usize::from(name_w)),
                theme.text(),
            )),
            Cell::from(Span::styled("<->", Style::default().fg(theme.correlated))),
            Cell::from(Span::styled(
                fit(&c.b_name, usize::from(name_w)),
                theme.text(),
            )),
            Cell::from(Span::styled(
                format!("{pct:>3}%"),
                Style::default().fg(theme.score_color(pct)),
            )),
            Cell::from(Span::styled(fit(basis, basis_w), theme.muted())),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(name_w),
            Constraint::Length(3),
            Constraint::Length(name_w),
            Constraint::Length(5),
            Constraint::Fill(1),
        ],
    )
    .header(
        Row::new(["A", "<->", "B", "SCORE", "BASIS"].map(Cell::from))
            .style(theme.muted().add_modifier(Modifier::BOLD)),
    )
    .column_spacing(1)
    .row_highlight_style(theme.selection().add_modifier(Modifier::BOLD));
    let mut state = TableState::new().with_selected(selected);
    frame.render_stateful_widget(table, inner, &mut state);
}

fn render_detail(frame: &mut Frame, area: Rect, app: &App, selected: Option<&Correlation>) {
    let theme = &app.theme;
    let block = panel_block(theme, "CORRELATION", false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let Some(c) = selected else {
        let hint = Line::from(Span::styled("no correlation selected", theme.muted()));
        let rect = centered_rect(inner.width, 1, inner);
        frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), rect);
        return;
    };

    let width = usize::from(inner.width).saturating_sub(1);
    let value_w = width.saturating_sub(KEY_WIDTH);
    let mut lines = Vec::new();
    lines.extend(entity_lines(app, "A", &c.a, &c.a_name, value_w));
    lines.extend(entity_lines(app, "B", &c.b, &c.b_name, value_w));
    lines.push(Line::default());
    let pct = c.score.pct();
    let mut score = vec![
        Span::styled(format!("{:<KEY_WIDTH$}", "SCORE"), theme.muted()),
        Span::styled(
            format!("{pct}%  "),
            Style::default()
                .fg(theme.score_color(pct))
                .add_modifier(Modifier::BOLD),
        ),
    ];
    score.extend(confidence_bar(theme, pct, 12).spans);
    lines.push(Line::from(score));
    lines.push(Line::from(Span::styled("BASIS", theme.muted())));
    let head_h = u16::try_from(lines.len()).unwrap_or(u16::MAX);

    let [head, basis] =
        Layout::vertical([Constraint::Length(head_h), Constraint::Fill(1)]).areas(inner);
    let text = Rect {
        x: head.x + 1,
        width: head.width.saturating_sub(1),
        ..head
    };
    frame.render_widget(Paragraph::new(lines), text);

    let basis_text = if c.basis.is_empty() {
        Span::styled("(no evidence)", theme.muted())
    } else {
        Span::styled(c.basis.clone(), theme.text())
    };
    let basis_rect = Rect {
        x: basis.x + 1,
        width: basis.width.saturating_sub(2),
        ..basis
    };
    frame.render_widget(
        Paragraph::new(Line::from(basis_text)).wrap(Wrap { trim: false }),
        basis_rect,
    );
}

/// `A  ◇ aws-alb` + KIND / ADDRESS / STATUS da entidade (se estiver no grafo).
fn entity_lines(
    app: &App,
    label: &str,
    id: &str,
    name: &str,
    value_w: usize,
) -> Vec<Line<'static>> {
    let theme = &app.theme;
    let Some(e) = app.graph.entity(id) else {
        return vec![
            kv_line(
                theme,
                label,
                Span::styled(
                    fit(name, value_w),
                    theme.text().add_modifier(Modifier::BOLD),
                ),
            ),
            kv_line(theme, "", Span::styled("(not in graph)", theme.muted())),
        ];
    };
    let status = e.status();
    let address = e
        .primary_address()
        .map_or_else(|| "—".to_string(), |a| a.display());
    vec![
        Line::from(vec![
            Span::styled(format!("{label:<KEY_WIDTH$}"), theme.muted()),
            Span::styled(format!("{} ", status_glyph(status)), theme.status(status)),
            Span::styled(
                fit(&e.name, value_w.saturating_sub(2)),
                theme.text().add_modifier(Modifier::BOLD),
            ),
        ]),
        kv_line(theme, "  KIND", Span::styled(e.kind.label(), theme.text())),
        kv_line(
            theme,
            "  ADDRESS",
            Span::styled(fit(&address, value_w), theme.text()),
        ),
        kv_line(
            theme,
            "  STATUS",
            Span::styled(status.label(), theme.status(status)),
        ),
    ]
}

/// Largura de coluna = máximo dos conteúdos, entre `min` e `max`.
fn col_width(widths: impl Iterator<Item = usize>, min: usize, max: usize) -> u16 {
    let w = widths.max().unwrap_or(0).clamp(min, max);
    u16::try_from(w).unwrap_or(u16::MAX)
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
