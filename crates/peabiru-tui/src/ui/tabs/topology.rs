//! Tab TOPOLOGY (ARCHITECTURE §6.2, "Outros tabs"): tabela de todas as
//! entidades do grafo (cursor = `app.selected_node`, na ordem de
//! `graph.entities()` — a mesma que `App::selected_entity` usa) sobre a
//! tabela de arestas.

use peabiru_core::{Entity, NodeStatus};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::App;
use crate::widgets::{centered_rect, panel_block, status_glyph, status_span};

/// Limites das colunas de texto livre (nomes, endereços).
const NAME_MAX: usize = 26;
const ADDR_MAX: usize = 24;
/// ADDRESS mais larga quando o painel tem folga (≥ `WIDE_PANEL` colunas).
const ADDR_MAX_WIDE: usize = 32;
const WIDE_PANEL: u16 = 130;
/// `◇ CORRELATED`.
const STATUS_W: u16 = 12;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let [top, bottom] =
        Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);
    render_entities(frame, top, app);
    render_edges(frame, bottom, app);
}

fn render_entities(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let entities: Vec<&Entity> = app.graph.entities().collect();
    let block = panel_block(theme, &format!("ENTITIES ({})", entities.len()), true);
    let inner = inset(block.inner(area));
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    if entities.is_empty() {
        hint(frame, inner, app, "no entities discovered");
        return;
    }

    let addr_max = if inner.width >= WIDE_PANEL {
        ADDR_MAX_WIDE
    } else {
        ADDR_MAX
    };
    let name_w = col_width(entities.iter().map(|e| e.name.width()), 4, NAME_MAX);
    let kind_w = col_width(entities.iter().map(|e| e.kind.label().len()), 4, 13);
    let prov_w = col_width(entities.iter().map(|e| e.provider.label().len()), 8, 10);
    let addr_w = col_width(
        entities
            .iter()
            .map(|e| e.primary_address().map_or(0, |a| a.display().width())),
        7,
        addr_max,
    );
    let fixed = name_w + kind_w + prov_w + addr_w + STATUS_W + 5;
    let sources_w = usize::from(inner.width.saturating_sub(fixed));

    let rows = entities.iter().map(|e| {
        let status = e.status();
        let address = e
            .primary_address()
            .map_or_else(String::new, |a| a.display());
        let sources = e.sources();
        let sources_text = if sources.is_empty() {
            "none".to_string()
        } else {
            format!("{} · {}", sources.len(), sources.join(", "))
        };
        Row::new(vec![
            Cell::from(Span::styled(
                fit(&e.name, usize::from(name_w)),
                theme.text(),
            )),
            Cell::from(Span::styled(e.kind.label(), theme.text())),
            Cell::from(Span::styled(e.provider.label(), theme.muted())),
            Cell::from(Span::styled(
                fit(&address, usize::from(addr_w)),
                theme.muted(),
            )),
            Cell::from(status_cell(app, status)),
            Cell::from(Span::styled(fit(&sources_text, sources_w), theme.muted())),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(name_w),
            Constraint::Length(kind_w),
            Constraint::Length(prov_w),
            Constraint::Length(addr_w),
            Constraint::Length(STATUS_W),
            Constraint::Fill(1),
        ],
    )
    .header(header(
        app,
        ["NAME", "KIND", "PROVIDER", "ADDRESS", "STATUS", "SOURCES"],
    ))
    .column_spacing(1)
    .row_highlight_style(theme.selection().add_modifier(Modifier::BOLD));
    let mut state = TableState::new().with_selected(app.selected_node % entities.len());
    frame.render_stateful_widget(table, inner, &mut state);
}

fn render_edges(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let edges: Vec<_> = app.graph.edges().collect();
    let block = panel_block(theme, &format!("EDGES ({})", edges.len()), false);
    let inner = inset(block.inner(area));
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    if edges.is_empty() {
        hint(frame, inner, app, "no edges discovered");
        return;
    }

    let name_of = |id: &str| -> String {
        app.graph
            .entity(id)
            .map_or_else(|| id.to_string(), |e| e.name.clone())
    };
    let name_w = col_width(
        edges
            .iter()
            .flat_map(|(_, e)| [name_of(&e.from).width(), name_of(&e.to).width()]),
        4,
        NAME_MAX,
    );
    let rows = edges.iter().map(|(_, e)| {
        let status = e.status();
        let pct = e.confidence().pct();
        Row::new(vec![
            Cell::from(Span::styled(
                fit(&name_of(&e.from), usize::from(name_w)),
                theme.text(),
            )),
            Cell::from(Span::styled(e.relation.arrow(), theme.muted())),
            Cell::from(Span::styled(
                fit(&name_of(&e.to), usize::from(name_w)),
                theme.text(),
            )),
            Cell::from(Span::styled(e.relation.label(), theme.muted())),
            Cell::from(status_cell(app, status)),
            Cell::from(Span::styled(
                format!("{pct:>3}%"),
                Style::default().fg(theme.score_color(pct)),
            )),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(name_w),
            Constraint::Length(1),
            Constraint::Length(name_w),
            Constraint::Length(11),
            Constraint::Length(STATUS_W),
            Constraint::Length(5),
        ],
    )
    .header(header(
        app,
        ["FROM", "→", "TO", "RELATION", "STATUS", "CONF%"],
    ))
    .column_spacing(1);
    frame.render_widget(table, inner);
}

/// Cabeçalho em `muted` negrito.
fn header<'a, const N: usize>(app: &App, titles: [&'static str; N]) -> Row<'a> {
    Row::new(titles.map(Cell::from)).style(app.theme.muted().add_modifier(Modifier::BOLD))
}

/// `● OBSERVED` na cor do status.
fn status_cell<'a>(app: &App, status: NodeStatus) -> Span<'a> {
    status_span(
        &app.theme,
        status,
        format!("{} {}", status_glyph(status), status.label()),
    )
}

fn hint(frame: &mut Frame, inner: Rect, app: &App, text: &str) {
    let line = Line::from(Span::styled(text.to_string(), app.theme.muted()));
    let rect = centered_rect(inner.width, 1, inner);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), rect);
}

/// Uma coluna de margem à esquerda (as tabelas não colam na borda).
fn inset(inner: Rect) -> Rect {
    Rect {
        x: inner.x.saturating_add(1),
        width: inner.width.saturating_sub(1),
        ..inner
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_width_clamps() {
        assert_eq!(col_width([3, 30].into_iter(), 4, 26), 26);
        assert_eq!(col_width([1].into_iter(), 4, 26), 4);
        assert_eq!(col_width(std::iter::empty(), 4, 26), 4);
        assert_eq!(fit("alb2c3.elb.amazonaws.com", 10), "alb2c3.el…");
    }
}
