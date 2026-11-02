//! Painel CORRELATIONS (ARCHITECTURE §6.2): título `CORRELATIONS (N)` e uma
//! linha `◇ a  <->  b  92%` por correlação do trace — score em `ok` (≥ 80),
//! `warn` (≥ 60) ou `error`; a linha selecionada ganha fundo `selection`
//! quando o painel tem foco; scroll mantém a seleção visível. Os nomes saem
//! inteiros sempre que cabem (se preciso com o separador compacto ` <-> `);
//! só depois disso são recortados com `…`.

use peabiru_core::NodeStatus;
use peabiru_trace::Correlation;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use super::node_details::{fit_line, truncate_str};
use crate::app::{App, Panel};
use crate::theme::Theme;
use crate::widgets;

/// Largura nominal das colunas `a` e `b` (mockup 156×50).
const A_WIDTH: usize = 14;
const B_WIDTH: usize = 12;
/// Largura mínima de cada coluna de nome em painéis estreitos.
const MIN_NAME_WIDTH: usize = 4;
/// `◇ ` antes do nome `a`.
const GLYPH_WIDTH: usize = 2;
/// Separador entre os nomes e a versão compacta, usada quando é o que falta
/// para os nomes caberem inteiros.
const ARROW: &str = "  <->  ";
const ARROW_COMPACT: &str = " <-> ";
/// Coluna do score: um espaço + `100%`.
const SCORE_WIDTH: usize = 5;

/// Colunas de uma linha: larguras de `a`/`b` e o separador entre elas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Columns {
    a: usize,
    b: usize,
    arrow: &'static str,
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let correlations: &[Correlation] = app
        .trace
        .as_ref()
        .map_or(&[], |t| t.correlations.as_slice());
    let focused = app.focus == Panel::Correlations;
    let title = format!("CORRELATIONS ({})", correlations.len());
    let block = widgets::panel_block(theme, &title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let content = inner.inner(Margin::new(1, 0));
    if content.width == 0 || content.height == 0 {
        return;
    }
    let width = usize::from(content.width);
    let rows = usize::from(content.height);

    if correlations.is_empty() {
        let msg = Line::from(Span::styled("no correlations", theme.muted()));
        frame.render_widget(Paragraph::new(fit_line(msg, width)), content);
        return;
    }

    let selected = app.selected_correlation.min(correlations.len() - 1);
    let offset = scroll_offset(selected, rows);
    let columns = Columns::fit(correlations, width);
    let lines: Vec<Line> = correlations
        .iter()
        .skip(offset)
        .take(rows)
        .map(|c| fit_line(row_line(theme, c, columns, width), width))
        .collect();
    frame.render_widget(Paragraph::new(lines), content);

    if focused {
        let y = content.y + u16::try_from(selected - offset).unwrap_or(0);
        let row = Rect::new(inner.x, y, inner.width, 1);
        frame.buffer_mut().set_style(row, theme.selection());
    }
}

/// `◇ a  <->  b  92%`: nomes recortados/preenchidos às larguras de
/// `columns` e score alinhado à direita no que sobra de `width`.
fn row_line(theme: &Theme, c: &Correlation, columns: Columns, width: usize) -> Line<'static> {
    let a = pad(&truncate_str(&c.a_name, columns.a), columns.a);
    let b = pad(&truncate_str(&c.b_name, columns.b), columns.b);
    let used = GLYPH_WIDTH + columns.a + columns.arrow.width() + columns.b;
    let pct = c.score.pct();
    let score = format!("{pct}%");
    let score_width = width.saturating_sub(used).max(score.len() + 1);
    Line::from(vec![
        Span::styled(
            widgets::status_glyph(NodeStatus::Correlated),
            Style::default().fg(theme.correlated),
        ),
        Span::raw(" "),
        Span::styled(a, theme.text()),
        Span::styled(columns.arrow, theme.muted()),
        Span::styled(b, theme.text()),
        Span::styled(
            format!("{score:>score_width$}"),
            Style::default().fg(theme.score_color(pct)),
        ),
    ])
}

impl Columns {
    /// Colunas para `width`: nomes inteiros (preenchidos até as larguras
    /// nominais quando sobra espaço) com o separador nominal ou, se só assim
    /// couberem, com o compacto; senão as larguras nominais e, em painéis
    /// estreitos, o que sobra (descontando glifo, seta e score) repartido na
    /// mesma proporção, com `MIN_NAME_WIDTH` por coluna.
    fn fit(correlations: &[Correlation], width: usize) -> Self {
        let widest = |f: fn(&Correlation) -> &str| {
            correlations.iter().map(|c| f(c).width()).max().unwrap_or(0)
        };
        let (max_a, max_b) = (widest(|c| &c.a_name), widest(|c| &c.b_name));
        let score = correlations
            .iter()
            .map(|c| 1 + format!("{}%", c.score.pct()).len())
            .max()
            .unwrap_or(SCORE_WIDTH);
        for arrow in [ARROW, ARROW_COMPACT] {
            let avail = width.saturating_sub(GLYPH_WIDTH + arrow.width() + score);
            if max_a + max_b <= avail {
                let a = max_a.max(A_WIDTH.min(avail - max_b));
                let b = max_b.max(B_WIDTH.min(avail - a));
                return Self { a, b, arrow };
            }
        }
        let avail = width.saturating_sub(GLYPH_WIDTH + ARROW.width() + SCORE_WIDTH);
        if avail >= A_WIDTH + B_WIDTH {
            return Self {
                a: A_WIDTH,
                b: B_WIDTH,
                arrow: ARROW,
            };
        }
        let a = (avail * A_WIDTH / (A_WIDTH + B_WIDTH)).max(MIN_NAME_WIDTH);
        let b = avail.saturating_sub(a).max(MIN_NAME_WIDTH);
        Self { a, b, arrow: ARROW }
    }
}

/// Primeiro índice visível para que `selected` caiba em `rows` linhas.
fn scroll_offset(selected: usize, rows: usize) -> usize {
    (selected + 1).saturating_sub(rows)
}

/// `s` preenchido com espaços até `width` colunas.
fn pad(s: &str, width: usize) -> String {
    format!("{s}{}", " ".repeat(width.saturating_sub(s.width())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use peabiru_core::Confidence;

    fn text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn scroll_keeps_selection_visible() {
        assert_eq!(scroll_offset(0, 3), 0);
        assert_eq!(scroll_offset(2, 3), 0);
        assert_eq!(scroll_offset(3, 3), 1);
        assert_eq!(scroll_offset(7, 3), 5);
        assert_eq!(scroll_offset(4, 1), 4);
    }

    fn corr(a: &str, b: &str, score: u8) -> Correlation {
        Correlation {
            a: a.into(),
            a_name: a.into(),
            b: b.into(),
            b_name: b.into(),
            score: Confidence::new(score),
            basis: String::new(),
        }
    }

    fn demo() -> Vec<Correlation> {
        vec![
            corr("aws-alb", "app-svc", 92),
            corr("web-frontend-7f9c8c", "orders-db", 81),
            corr("istio-ingressgateway", "redis", 73),
        ]
    }

    #[test]
    fn columns_keep_full_names_while_they_fit() {
        let nominal = Columns {
            a: A_WIDTH,
            b: B_WIDTH,
            arrow: ARROW,
        };
        // Nomes curtos: larguras nominais do mockup.
        assert_eq!(Columns::fit(&[corr("aws-alb", "app-svc", 92)], 40), nominal);
        assert_eq!(Columns::fit(&demo(), 60).arrow, ARROW);
        assert_eq!(
            (Columns::fit(&demo(), 60).a, Columns::fit(&demo(), 60).b),
            (20, 12)
        );
        // 156x50 (40 colunas): os nomes do demo só cabem com ` <-> `.
        assert_eq!(
            Columns::fit(&demo(), 40),
            Columns {
                a: 20,
                b: 9,
                arrow: ARROW_COMPACT
            }
        );
        // Sem espaço para os nomes inteiros: nominal, depois proporcional.
        let long = vec![corr(&"a".repeat(30), &"b".repeat(30), 50)];
        assert_eq!(Columns::fit(&long, 60), nominal);
        let c = Columns::fit(&demo(), 39);
        assert_eq!((c.a + c.b, c.arrow), (25, ARROW));
        let c = Columns::fit(&demo(), 24);
        assert_eq!((c.a + c.b, c.arrow), (10, ARROW));
        assert!(c.a >= MIN_NAME_WIDTH && c.b >= MIN_NAME_WIDTH);
        let c = Columns::fit(&demo(), 5);
        assert_eq!((c.a, c.b), (MIN_NAME_WIDTH, MIN_NAME_WIDTH));
        assert_eq!(Columns::fit(&[], 40), nominal);
    }

    #[test]
    fn row_is_exactly_the_panel_width() {
        let theme = Theme::default_dark();
        let nominal = Columns {
            a: A_WIDTH,
            b: B_WIDTH,
            arrow: ARROW,
        };
        let c = corr("aws-alb", "app-svc", 92);
        let s = text(&row_line(&theme, &c, nominal, 40));
        assert_eq!(s.width(), 40, "{s:?}");
        assert_eq!(
            s,
            format!(
                "◇ aws-alb{}  <->  app-svc{}  92%",
                " ".repeat(A_WIDTH - 7),
                " ".repeat(B_WIDTH - 7)
            )
        );
        let verified = Correlation {
            score: Confidence::VERIFIED,
            ..c.clone()
        };
        let s = text(&row_line(&theme, &verified, nominal, 40));
        assert_eq!(s.width(), 40, "{s:?}");
        assert!(s.ends_with(" 100%"), "{s:?}");
        let long = corr("istio-ingressgateway", "orders-database", 92);
        let s = text(&row_line(&theme, &long, nominal, 40));
        assert_eq!(s.width(), 40, "{s:?}");
        assert_eq!(s, "◇ istio-ingress…  <->  orders-data…  92%");
        // Com as colunas do demo a 40, nada é recortado.
        let s = text(&row_line(&theme, &demo()[2], Columns::fit(&demo(), 40), 40));
        assert_eq!(s, "◇ istio-ingressgateway <-> redis     73%");
        assert_eq!(s.width(), 40);
    }
}
