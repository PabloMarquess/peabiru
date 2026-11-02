//! Testes do renderer do TRACE GRAPH (ARCHITECTURE §6.3) sobre o trace demo.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use peabiru_discovery::demo;
use peabiru_tui::app::Panel;
use peabiru_tui::ui::{self, graph};
use peabiru_tui::{render_to_string, App, Theme};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::Terminal;
use unicode_width::UnicodeWidthStr;

const HOPS: [&str; 5] = [
    "web-frontend-7f9c8c",
    "kube-proxy",
    "istio-ingressgateway",
    "aws-alb",
    "app-svc",
];
const BRANCHES: [&str; 4] = ["nat-gw", "redis", "orders-db", "payments-svc"];

fn traced_app() -> App {
    let mut app = App::new(
        demo::graph(),
        demo::default_request(),
        demo::environment(),
        Theme::default_dark(),
    );
    app.run_trace();
    assert!(app.trace.is_some());
    app
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Desenha `app` num `TestBackend` e devolve o buffer final.
fn draw(app: &App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}

/// Área do painel TRACE GRAPH (com borda) num terminal `width`×`height`.
fn graph_rect(width: u16, height: u16) -> Rect {
    ui::trace_layout(ui::layout(Rect::new(0, 0, width, height)).main).graph
}

/// Texto de cada linha de `rect` dentro de `buf` (só o painel, sem os vizinhos).
fn panel_rows(buf: &Buffer, rect: Rect) -> Vec<String> {
    (rect.top()..rect.bottom())
        .map(|y| {
            (rect.left()..rect.right())
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

/// Linha (dentro do painel) e coluna (absoluta) do primeiro caractere de `needle`.
fn locate(buf: &Buffer, rect: Rect, needle: &str) -> Option<(usize, u16)> {
    panel_rows(buf, rect)
        .iter()
        .enumerate()
        .find_map(|(row, line)| {
            line.find(needle).map(|byte| {
                let col = u16::try_from(line[..byte].chars().count()).unwrap();
                (row, rect.x + col)
            })
        })
}

#[test]
fn hops_render_top_to_bottom_each_on_its_own_line() {
    let app = traced_app();
    let buf = draw(&app, 156, 50);
    let rows = panel_rows(&buf, graph_rect(156, 50));

    let mut last_row = None;
    for name in HOPS {
        let matches: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, l)| l.contains(name))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "{name:?} should be on exactly one line: {rows:#?}"
        );
        let row = matches[0];
        if let Some(prev) = last_row {
            assert!(
                row > prev,
                "{name:?} (row {row}) must be below the previous hop (row {prev})"
            );
        }
        last_row = Some(row);
    }
}

#[test]
fn branches_render_next_to_their_anchor() {
    let app = traced_app();
    let buf = draw(&app, 156, 50);
    let rect = graph_rect(156, 50);
    let rows = panel_rows(&buf, rect);
    for name in BRANCHES {
        assert!(
            rows.iter().any(|l| l.contains(name)),
            "missing {name:?}:\n{}",
            rows.join("\n")
        );
    }
    let (istio, _) = locate(&buf, rect, "istio-ingressgateway").unwrap();
    let (nat_gw, _) = locate(&buf, rect, "nat-gw").unwrap();
    assert!(
        istio.abs_diff(nat_gw) <= 3,
        "istio row {istio}, nat-gw row {nat_gw}"
    );
    // O "L" tracejado sai da linha do istio e desce até o nat-gw.
    assert!(rows[istio].contains("╭╌╌"), "{}", rows[istio]);
    assert!(rows[istio + 1].contains('╎'), "{}", rows[istio + 1]);
}

#[test]
fn frame_never_exceeds_terminal_width() {
    let app = traced_app();
    for (w, h) in [(156u16, 50u16), (100, 30), (120, 40)] {
        let out = render_to_string(&app, w, h, false);
        assert_eq!(out.lines().count(), usize::from(h));
        for line in out.lines() {
            assert!(
                line.width() <= usize::from(w),
                "{w}x{h}: line wider than the terminal ({}): {line:?}",
                line.width()
            );
        }
        let rect = graph_rect(w, h);
        let rows = panel_rows(&draw(&app, w, h), rect);
        assert!(
            rows.iter().any(|l| l.contains("web-frontend-7f9c8c")),
            "{w}x{h}: first hop visible with the default selection:\n{}",
            rows.join("\n")
        );
        // O último hop fica visível ao ser selecionado (a 100x30 o grafo não
        // cabe inteiro e rola; nos outros tamanhos já está à vista).
        let mut selected = app.clone();
        selected.selected_node = 4;
        let rows = panel_rows(&draw(&selected, w, h), rect);
        assert!(
            rows.iter().any(|l| l.contains("app-svc")),
            "{w}x{h}:\n{}",
            rows.join("\n")
        );
        if h >= 40 {
            let rows = panel_rows(&draw(&app, w, h), rect);
            assert!(
                rows.iter().any(|l| l.contains("app-svc")),
                "{w}x{h} fits without scrolling"
            );
        }
    }
}

#[test]
fn main_path_connectors_are_green_and_aligned() {
    let app = traced_app();
    let buf = draw(&app, 156, 50);
    let rect = graph_rect(156, 50);
    let (row, name_col) = locate(&buf, rect, "web-frontend-7f9c8c").unwrap();
    let glyph_col = name_col - 3;
    let y = rect.y + u16::try_from(row).unwrap();
    assert_eq!(buf[(glyph_col, y)].symbol(), "●");
    assert_eq!(buf[(glyph_col, y)].fg, app.theme.observed);
    // Endereço em muted logo abaixo do nome; seta e ponta na coluna do glifo.
    assert_eq!(buf[(name_col, y + 1)].symbol(), "1");
    assert_eq!(buf[(name_col, y + 1)].fg, app.theme.muted);
    assert_eq!(buf[(glyph_col, y + 2)].symbol(), "│");
    assert_eq!(buf[(glyph_col, y + 2)].fg, app.theme.observed);
    assert_eq!(buf[(glyph_col, y + 3)].symbol(), "▼");
    let (next_row, next_col) = locate(&buf, rect, "kube-proxy").unwrap();
    assert_eq!(next_row, row + 4, "4 lines per hop");
    assert_eq!(next_col, name_col, "hops share the same column");
    // aws-alb mostra os dois endereços em linhas consecutivas.
    let (alb_row, alb_col) = locate(&buf, rect, "alb2c3.elb.amazonaws.com").unwrap();
    let (ip_row, ip_col) = locate(&buf, rect, "52.95.23.10:443").unwrap();
    assert_eq!(ip_row, alb_row + 1);
    assert_eq!(ip_col, alb_col);
}

#[test]
fn down_twice_selects_istio_and_renders_it_bold() {
    let mut app = traced_app();
    assert_eq!(app.focus, Panel::Graph);
    app.on_key(key(KeyCode::Down));
    app.on_key(key(KeyCode::Down));
    assert_eq!(app.selected_node, 2);
    assert_eq!(app.graph_nodes()[2].name, "istio-ingressgateway");

    let buf = draw(&app, 156, 50);
    let rect = graph_rect(156, 50);
    let (row, col) = locate(&buf, rect, "istio-ingressgateway").unwrap();
    let y = rect.y + u16::try_from(row).unwrap();
    let name = &buf[(col, y)];
    assert!(
        name.modifier.contains(Modifier::BOLD),
        "selected name is bold: {name:?}"
    );
    let glyph = &buf[(col - 3, y)];
    assert_eq!(glyph.symbol(), "●");
    assert!(
        glyph.modifier.contains(Modifier::REVERSED),
        "selected glyph is reversed: {glyph:?}"
    );

    // Os demais nós não ficam em negrito.
    let (row, col) = locate(&buf, rect, "kube-proxy").unwrap();
    let other = &buf[(col, rect.y + u16::try_from(row).unwrap())];
    assert!(!other.modifier.contains(Modifier::BOLD));
    assert!(!buf[(col - 3, rect.y + u16::try_from(row).unwrap())]
        .modifier
        .contains(Modifier::REVERSED));
}

#[test]
fn small_terminal_scrolls_to_keep_selection_visible() {
    let mut app = traced_app();
    let rect = graph_rect(100, 30);
    let inner_h = rect.height - 2;
    assert!(
        graph::layout_height(&app, rect.width - 2) > inner_h,
        "the demo graph must not fit at 100x30 for this test to be meaningful"
    );

    let rows = panel_rows(&draw(&app, 100, 30), rect);
    assert!(
        rows.iter().any(|l| l.contains("web-frontend-7f9c8c")),
        "{}",
        rows.join("\n")
    );
    assert!(rows.iter().any(|l| l.contains("↓ more")));

    app.on_key(key(KeyCode::Char('G')));
    assert_eq!(app.selected_node, app.graph_nodes().len() - 1);
    let rows = panel_rows(&draw(&app, 100, 30), rect);
    assert!(
        rows.iter().any(|l| l.contains("payments-svc")),
        "{}",
        rows.join("\n")
    );
    assert!(rows.iter().any(|l| l.contains("↑ more")));

    app.on_key(key(KeyCode::Char('g')));
    let rows = panel_rows(&draw(&app, 100, 30), rect);
    assert!(
        rows.iter().any(|l| l.contains("web-frontend-7f9c8c")),
        "{}",
        rows.join("\n")
    );
}

#[test]
fn without_a_trace_shows_the_hint() {
    let app = App::new(
        demo::graph(),
        demo::default_request(),
        demo::environment(),
        Theme::default_dark(),
    );
    let rows = panel_rows(&draw(&app, 156, 50), graph_rect(156, 50));
    assert!(
        rows.iter().any(|l| l.contains("press r to run a trace")),
        "{}",
        rows.join("\n")
    );
    assert!(!rows.iter().any(|l| l.contains('●')));
    assert_eq!(graph::layout_height(&app, 69), 0);
}
