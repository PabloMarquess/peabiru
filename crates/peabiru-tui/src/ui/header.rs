//! Header (3 linhas): logo | tabs | conexão (ARCHITECTURE §6.2).
//!
//! Três blocos arredondados lado a lado, cada um com uma linha interna. O
//! logo e a conexão têm a largura do próprio conteúdo; as tabs ocupam o
//! resto. Em terminais estreitos o conteúdo degrada em passos (separadores
//! simples, dicas compactas, sem endpoint, sem subtítulo, rótulos abreviados)
//! em vez de ser recortado pela borda.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

use crate::app::{App, Tab};
use crate::theme::Theme;

/// Dicas `? Help  q Quit` à direita do bloco de tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hints {
    Full,
    Compact,
    Hidden,
}

/// Um passo de degradação do header (do mais completo ao mais compacto).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Variant {
    /// `NETWORK TRACING` ao lado do logo.
    logo_sub: bool,
    /// Endpoint ao lado de `connected ●`.
    endpoint: bool,
    /// Separador entre tabs.
    sep: &'static str,
    hints: Hints,
    /// Rótulos abreviados (`Correl.`, `Observ.`).
    short_labels: bool,
}

const fn variant(
    logo_sub: bool,
    endpoint: bool,
    sep: &'static str,
    hints: Hints,
    short_labels: bool,
) -> Variant {
    Variant {
        logo_sub,
        endpoint,
        sep,
        hints,
        short_labels,
    }
}

/// Variantes em ordem de preferência; a primeira que cabe é usada.
const VARIANTS: [Variant; 9] = [
    variant(true, true, "  ", Hints::Full, false),
    variant(true, true, " ", Hints::Full, false),
    variant(true, true, " ", Hints::Compact, false),
    variant(true, false, " ", Hints::Full, false),
    variant(true, false, " ", Hints::Compact, false),
    variant(true, false, " ", Hints::Hidden, false),
    variant(false, false, " ", Hints::Compact, false),
    variant(false, false, " ", Hints::Hidden, false),
    variant(false, false, " ", Hints::Hidden, true),
];

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let variant = pick_variant(app, area.width);
    let logo = logo_line(theme, variant.logo_sub);
    let conn = connection_line(app, variant.endpoint);
    let tabs = tabs_line(app, variant);
    let hints = hints_line(theme, variant.hints);

    let [logo_area, tabs_area, conn_area] = Layout::horizontal([
        Constraint::Length(block_width(&logo)),
        Constraint::Fill(1),
        Constraint::Length(block_width(&conn)),
    ])
    .areas(area);

    render_chrome(frame, logo_area, theme, logo);
    render_chrome(frame, conn_area, theme, conn);

    let block = chrome_block(theme);
    let inner = block.inner(tabs_area);
    frame.render_widget(block, tabs_area);
    let [left, right] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(cols(hints.width()))])
            .areas(inner);
    frame.render_widget(Paragraph::new(tabs), left);
    frame.render_widget(Paragraph::new(hints.right_aligned()), right);
}

/// Primeira variante cujo conteúdo cabe em `width` colunas (a última se
/// nenhuma couber).
fn pick_variant(app: &App, width: u16) -> Variant {
    let theme = &app.theme;
    VARIANTS
        .into_iter()
        .find(|v| {
            let sides = usize::from(block_width(&logo_line(theme, v.logo_sub)))
                + usize::from(block_width(&connection_line(app, v.endpoint)));
            let tabs_inner = usize::from(width).saturating_sub(sides + 2);
            tabs_line(app, *v).width() + hints_line(theme, v.hints).width() <= tabs_inner
        })
        .unwrap_or(VARIANTS[VARIANTS.len() - 1])
}

/// Bloco arredondado sem título, na cor `border` (o header nunca tem foco).
fn chrome_block<'a>(theme: &Theme) -> Block<'a> {
    let mut block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border(false));
    if theme.paint_bg {
        block = block.style(Style::default().bg(theme.panel_bg));
    }
    block
}

fn render_chrome(frame: &mut Frame, area: Rect, theme: &Theme, line: Line<'_>) {
    let block = chrome_block(theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(line), inner);
}

/// ` PEABIRU  NETWORK TRACING ` (subtítulo opcional).
fn logo_line<'a>(theme: &Theme, sub: bool) -> Line<'a> {
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(
            "PEABIRU",
            Style::default().fg(theme.logo).add_modifier(Modifier::BOLD),
        ),
    ];
    if sub {
        spans.push(Span::styled(
            "  NETWORK TRACING",
            Style::default().fg(theme.logo_sub),
        ));
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

/// ` connected ● [YOLO]  peabiru.local:443 ` (endpoint opcional).
fn connection_line<'a>(app: &App, endpoint: bool) -> Line<'a> {
    let theme = &app.theme;
    let env = &app.environment;
    let mut spans = vec![Span::raw(" ")];
    if env.connected {
        spans.push(Span::styled("connected ●", Style::default().fg(theme.ok)));
    } else {
        spans.push(Span::styled(
            "disconnected ○",
            Style::default().fg(theme.error),
        ));
    }
    if app.yolo {
        spans.push(Span::styled(
            " YOLO",
            Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
        ));
    }
    if endpoint {
        spans.push(Span::styled(format!("  {}", env.endpoint), theme.muted()));
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

/// ` [1] Trace  [2] Topology …`: tab ativa em negrito sublinhado na cor `title`.
fn tabs_line<'a>(app: &App, v: Variant) -> Line<'a> {
    let theme = &app.theme;
    let active_style = Style::default()
        .fg(theme.title)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let mut spans = vec![Span::raw(" ")];
    for (i, tab) in Tab::ALL.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(v.sep));
        }
        spans.push(Span::styled(format!("[{}]", tab.number()), theme.key()));
        let label = if v.short_labels {
            short_label(tab)
        } else {
            tab.label()
        };
        let style = if tab == app.tab {
            active_style
        } else {
            theme.text()
        };
        spans.push(Span::styled(format!(" {label}"), style));
    }
    Line::from(spans)
}

/// `? Help  q Quit ` (com espaço final, para não encostar na borda).
fn hints_line<'a>(theme: &Theme, hints: Hints) -> Line<'a> {
    match hints {
        Hints::Full => Line::from(vec![
            Span::styled("?", theme.key()),
            Span::styled(" Help", theme.text()),
            Span::raw("  "),
            Span::styled("q", theme.key()),
            Span::styled(" Quit ", theme.text()),
        ]),
        Hints::Compact => Line::from(vec![
            Span::styled("?", theme.key()),
            Span::styled(" Help ", theme.text()),
            Span::styled("q", theme.key()),
            Span::styled(" Quit ", theme.text()),
        ]),
        Hints::Hidden => Line::default(),
    }
}

fn short_label(tab: Tab) -> &'static str {
    match tab {
        Tab::Correlations => "Correl.",
        Tab::Observability => "Observ.",
        other => other.label(),
    }
}

/// Largura de um bloco cujo conteúdo é `line` (+ 2 bordas).
fn block_width(line: &Line<'_>) -> u16 {
    cols(line.width() + 2)
}

fn cols(w: usize) -> u16 {
    u16::try_from(w).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use peabiru_discovery::demo;

    fn app() -> App {
        App::new(
            demo::graph(),
            demo::default_request(),
            demo::environment(),
            Theme::default_dark(),
        )
    }

    #[test]
    fn reference_width_uses_the_full_variant() {
        assert_eq!(pick_variant(&app(), 156), VARIANTS[0]);
    }

    #[test]
    fn yolo_at_reference_width_only_tightens_separators() {
        let mut app = app();
        app.yolo = true;
        let v = pick_variant(&app, 156);
        assert_eq!(v.sep, " ");
        assert!(v.logo_sub && v.endpoint);
        assert_eq!(v.hints, Hints::Full);
    }

    #[test]
    fn minimum_width_degrades_but_keeps_labels() {
        let v = pick_variant(&app(), 100);
        assert!(!v.short_labels);
        assert!(!v.endpoint && !v.logo_sub);
        assert_eq!(v.hints, Hints::Hidden);
        let mut app = app();
        app.yolo = true;
        assert!(pick_variant(&app, 100).short_labels);
    }

    #[test]
    fn content_widths() {
        let t = Theme::default_dark();
        assert_eq!(logo_line(&t, true).width(), 26);
        assert_eq!(logo_line(&t, false).width(), 9);
        assert_eq!(hints_line(&t, Hints::Full).width(), 15);
        assert_eq!(hints_line(&t, Hints::Hidden).width(), 0);
        let mut app = app();
        assert_eq!(connection_line(&app, true).width(), 32);
        app.environment.connected = false;
        assert_eq!(connection_line(&app, false).width(), 16);
        assert_eq!(tabs_line(&app, VARIANTS[0]).width(), 75);
    }
}
