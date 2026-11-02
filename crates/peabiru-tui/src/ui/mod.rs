//! Layout (ARCHITECTURE §6.2) e despacho para os painéis. Cada painel vive
//! em seu próprio arquivo com a assinatura
//! `pub fn render(frame: &mut Frame, area: Rect, app: &App)`.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, Tab};
use crate::widgets;

pub mod correlations;
pub mod details;
pub mod events;
pub mod footer;
pub mod graph;
pub mod header;
pub mod help;
pub mod keymap;
pub mod legend;
pub mod node_details;
pub mod tabs;
pub mod trace_summary;
pub mod trace_target;

/// Tamanho mínimo suportado.
pub const MIN_WIDTH: u16 = 100;
pub const MIN_HEIGHT: u16 = 30;

/// Alturas fixas da coluna esquerda do Tab Trace (com bordas): TRACE TARGET
/// (alvo, PORT, FROM, separador, ação) e TRACE SUMMARY (8 linhas de conteúdo).
pub const TRACE_TARGET_HEIGHT: u16 = 7;
pub const TRACE_SUMMARY_HEIGHT: u16 = 10;
/// Abaixo disto a LEGEND não mostra nem uma entrada: suas linhas vão para o
/// TRACE SUMMARY e o painel não é desenhado.
pub const LEGEND_MIN_HEIGHT: u16 = 3;

/// Áreas da tela, calculadas por `layout()` (útil para testes e para o
/// renderer do grafo saber onde está).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Areas {
    pub header: Rect,
    pub main: Rect,
    pub events: Rect,
    pub keymap: Rect,
    pub footer: Rect,
}

/// Áreas do Tab Trace dentro de `Areas::main`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceAreas {
    pub trace_target: Rect,
    pub trace_summary: Rect,
    pub legend: Rect,
    pub graph: Rect,
    pub node_details: Rect,
    pub correlations: Rect,
}

/// Header (3) / principal / rodapé de painéis (8: EVENTS 66% + KEYMAP 34%) / status (1).
pub fn layout(area: Rect) -> Areas {
    let [header, main, bottom, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(8),
        Constraint::Length(1),
    ])
    .areas(area);
    let [events, keymap] =
        Layout::horizontal([Constraint::Percentage(66), Constraint::Percentage(34)]).areas(bottom);
    Areas {
        header,
        main,
        events,
        keymap,
        footer,
    }
}

/// Colunas 26/46/28; esquerda TRACE TARGET (7) / TRACE SUMMARY (10) / LEGEND
/// (restante; com menos de `LEGEND_MIN_HEIGHT` linhas, altura 0 e o TRACE
/// SUMMARY fica com elas); direita NODE DETAILS (min 20) / CORRELATIONS
/// (restante).
pub fn trace_layout(main: Rect) -> TraceAreas {
    let [left, graph, right] = Layout::horizontal([
        Constraint::Percentage(26),
        Constraint::Percentage(46),
        Constraint::Percentage(28),
    ])
    .areas(main);
    let [trace_target, mut trace_summary, mut legend] = Layout::vertical([
        Constraint::Length(TRACE_TARGET_HEIGHT),
        Constraint::Length(TRACE_SUMMARY_HEIGHT),
        Constraint::Fill(1),
    ])
    .areas(left);
    if legend.height < LEGEND_MIN_HEIGHT {
        trace_summary.height += legend.height;
        legend.height = 0;
    }
    let [node_details, correlations] =
        Layout::vertical([Constraint::Min(20), Constraint::Fill(1)]).areas(right);
    TraceAreas {
        trace_target,
        trace_summary,
        legend,
        graph,
        node_details,
        correlations,
    }
}

/// Desenha um frame completo.
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    widgets::fill_bg(frame, area, &app.theme);
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        render_too_small(frame, area, app);
        return;
    }

    let areas = layout(area);
    header::render(frame, areas.header, app);
    match app.tab {
        Tab::Trace => {
            let t = trace_layout(areas.main);
            trace_target::render(frame, t.trace_target, app);
            trace_summary::render(frame, t.trace_summary, app);
            if t.legend.height > 0 {
                legend::render(frame, t.legend, app);
            }
            graph::render(frame, t.graph, app);
            node_details::render(frame, t.node_details, app);
            correlations::render(frame, t.correlations, app);
        }
        Tab::Topology => tabs::topology::render(frame, areas.main, app),
        Tab::Correlations => tabs::correlations::render(frame, areas.main, app),
        Tab::Observability => tabs::observability::render(frame, areas.main, app),
        Tab::Settings => tabs::settings::render(frame, areas.main, app),
    }
    events::render(frame, areas.events, app);
    keymap::render(frame, areas.keymap, app);
    footer::render(frame, areas.footer, app);

    if app.help_open {
        help::render(frame, area, app);
    }
    if app.details_open {
        details::render(frame, area, app);
    }
}

fn render_too_small(frame: &mut Frame, area: Rect, app: &App) {
    let msg = Line::from(vec![
        Span::styled("terminal too small: need ", app.theme.text()),
        Span::styled(format!("{MIN_WIDTH}x{MIN_HEIGHT}"), app.theme.key()),
        Span::styled(
            format!(" (have {}x{})", area.width, area.height),
            app.theme.muted(),
        ),
    ]);
    let rect = widgets::centered_rect(area.width, 1, area);
    frame.render_widget(Paragraph::new(msg).alignment(Alignment::Center), rect);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_layout_156x50() {
        let a = layout(Rect::new(0, 0, 156, 50));
        assert_eq!(a.header.height, 3);
        assert_eq!(a.main.height, 38);
        assert_eq!(a.events.height, 8);
        assert_eq!(a.keymap.height, 8);
        assert_eq!(a.footer.height, 1);
        assert_eq!(a.events.width + a.keymap.width, 156);
        let t = trace_layout(a.main);
        assert_eq!(t.trace_target.height, 7);
        assert_eq!(t.trace_summary.height, 10);
        assert_eq!(t.legend.height, 21);
        assert_eq!(t.legend.bottom(), a.main.bottom());
        assert!(t.node_details.height >= 20, "{:?}", t.node_details);
        assert_eq!(t.node_details.height + t.correlations.height, 38);
        assert_eq!(
            t.trace_target.width + t.graph.width + t.node_details.width,
            156
        );
    }

    #[test]
    fn minimum_layout_100x30() {
        let a = layout(Rect::new(0, 0, 100, 30));
        let t = trace_layout(a.main);
        assert_eq!(a.main.height, 18);
        // 7 + 10 deixam 1 linha: não dá para a LEGEND, vai para o SUMMARY.
        assert_eq!(t.trace_target.height, 7);
        assert_eq!(t.trace_summary.height, 11);
        assert_eq!(t.legend.height, 0);
        assert_eq!(t.trace_summary.bottom(), a.main.bottom());
        assert!(t.node_details.height >= 18);
    }

    #[test]
    fn legend_keeps_its_rows_once_it_can_show_an_entry() {
        // main = 40 - 12 = 28 → 7 + 10 + 11.
        let a = layout(Rect::new(0, 0, 120, 40));
        let t = trace_layout(a.main);
        assert_eq!(t.legend.height, 11);
        // main = 20: 7 + 10 + 3 → a LEGEND mostra uma entrada.
        let t = trace_layout(Rect::new(0, 0, 156, 20));
        assert_eq!(t.legend.height, LEGEND_MIN_HEIGHT);
        assert_eq!(t.trace_summary.height, 10);
    }
}
