//! Painéis da coluna direita (NODE DETAILS e CORRELATIONS) no layout de
//! referência 156×50 e no mínimo 100×30.

use peabiru_discovery::demo;
use peabiru_tui::app::Panel;
use peabiru_tui::ui;
use peabiru_tui::{render_to_string, App, Theme};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

fn app() -> App {
    App::new(
        demo::graph(),
        demo::default_request(),
        demo::environment(),
        Theme::default_dark(),
    )
}

fn select_node(app: &mut App, name: &str) {
    let idx = app
        .graph_nodes()
        .iter()
        .position(|n| n.name == name)
        .unwrap_or_else(|| panic!("node {name} not in graph_nodes()"));
    app.selected_node = idx;
}

/// Só as colunas `[x0, x1)` de cada linha do frame (o texto de outros painéis
/// na mesma linha não conta).
fn column(frame: &str, x0: usize, x1: usize) -> String {
    frame
        .lines()
        .map(|l| l.chars().skip(x0).take(x1 - x0).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn right_column(app: &App, width: u16, height: u16) -> String {
    let areas = ui::layout(Rect::new(0, 0, width, height));
    let t = ui::trace_layout(areas.main);
    let frame = render_to_string(app, width, height, false);
    assert!(frame
        .lines()
        .all(|l| l.chars().count() <= usize::from(width)));
    column(
        &frame,
        usize::from(t.node_details.x),
        usize::from(t.node_details.right()),
    )
}

#[test]
fn node_details_and_correlations_for_aws_alb() {
    let mut app = app();
    app.run_trace();
    select_node(&mut app, "aws-alb");
    let right = right_column(&app, 156, 50);
    for needle in [
        "NODE DETAILS",
        "aws-alb",
        "CORRELATED",
        "ADDRESS",
        "alb2c3.elb.amazonaws.com",
        "IP:PORT",
        "52.95.23.10:443",
        "TYPE",
        "load_balancer (aws_alb)",
        "AS16509",
        "REGION",
        "us-east-1",
        "STATUS",
        "reachable",
        "SOURCE",
        "2 sensors",
        "LAST SEEN",
        "s ago",
        "CONFIDENCE",
        "EVIDENCE (2)",
        "eBPF (node/ip-10-0-1-25)",
        "VPC Flow Logs",
        "[d] view details",
        "[y] yolo mode",
        "CORRELATIONS (3)",
        "<->",
        "92%",
        "81%",
        "73%",
    ] {
        assert!(right.contains(needle), "missing {needle:?} in:\n{right}");
    }
    // Nada do NODE DETAILS é truncado no layout de referência.
    let (details, correlations) = right.split_at(right.find("CORRELATIONS").expect("panel"));
    assert!(!details.contains("…"), "{details}");
    // Ordem das correlações = ordem do trace; nomes inteiros (separador
    // compacto ` <-> `); score alinhado à direita.
    let a = correlations
        .find("aws-alb              <-> app-svc   92%")
        .expect("row 1");
    let b = correlations
        .find("web-frontend-7f9c8c  <-> orders-db 81%")
        .expect("row 2");
    let c = correlations
        .find("istio-ingressgateway <-> redis     73%")
        .expect("row 3");
    assert!(a < b && b < c, "{correlations}");
    assert!(!correlations.contains('…'), "{correlations}");
    for row in correlations.lines().filter(|l| l.contains("<->")) {
        assert!(row.trim_end().ends_with("% │"), "{row:?}");
    }
    // Rodapé na última linha interna do NODE DETAILS.
    let lines: Vec<&str> = right.lines().collect();
    let footer = lines
        .iter()
        .position(|l| l.contains("[d] view details"))
        .expect("footer");
    assert!(
        lines[footer + 1].starts_with('╰'),
        "footer is pinned:\n{right}"
    );
}

#[test]
fn node_details_without_selection_and_no_correlations() {
    let app = app();
    let right = right_column(&app, 156, 50);
    assert!(right.contains("select a node in the graph"), "{right}");
    assert!(right.contains("CORRELATIONS (0)"), "{right}");
    assert!(right.contains("no correlations"), "{right}");
    assert!(!right.contains("EVIDENCE"), "{right}");
}

#[test]
fn every_graph_node_renders_without_overflow() {
    let mut app = app();
    app.run_trace();
    for idx in 0..app.graph_nodes().len() {
        app.selected_node = idx;
        for (w, h) in [(156, 50), (100, 30), (120, 40)] {
            let right = right_column(&app, w, h);
            assert!(right.contains("NODE DETAILS"), "{right}");
            assert!(right.contains("EVIDENCE ("), "{right}");
            assert!(right.contains("[d]"), "{right}");
        }
    }
}

#[test]
fn narrow_panels_truncate_with_ellipsis() {
    let mut app = app();
    app.run_trace();
    select_node(&mut app, "aws-alb");
    // 100x30: a coluna direita tem 24 colunas de conteúdo (CORRELATIONS
    // fica sem altura, pois NODE DETAILS ocupa o mínimo de 20 linhas) e o corpo
    // só cabe sem a linha em branco do cabeçalho.
    let right = right_column(&app, 100, 30);
    assert!(right.contains("ADDRESS      alb2c3.elb…"), "{right}");
    assert!(right.contains("● eBPF (node/ip-10-0-1-…"), "{right}");
    assert!(right.contains("● VPC Flow Logs"), "{right}");
    assert!(!right.contains("more"), "{right}");
    assert!(right.contains("[d] details  [y] yolo"), "{right}");
    let lines: Vec<&str> = right.lines().collect();
    let header = lines
        .iter()
        .position(|l| l.contains("[CORRELATED]"))
        .expect("header");
    assert!(
        lines[header + 1].contains("ADDRESS"),
        "blank line dropped:\n{right}"
    );
    // 120x40: nomes das correlações encurtados, score ainda visível.
    let right = right_column(&app, 120, 40);
    assert!(right.contains("<->  app-svc   92%"), "{right}");
    assert!(right.contains("web-fro…  <->  orders-…  81%"), "{right}");
    assert!(right.contains("istio-i…  <->  redis     73%"), "{right}");
}

#[test]
fn selected_correlation_row_gets_selection_background() {
    let mut app = app();
    app.run_trace();
    app.focus = Panel::Correlations;
    app.selected_correlation = 1;

    let size = Rect::new(0, 0, 156, 50);
    let mut terminal = Terminal::new(TestBackend::new(156, 50)).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer();
    let c = ui::trace_layout(ui::layout(size).main).correlations;
    let selection = app.theme.selection;
    let x = c.x + 1;
    let row = |i: u16| c.y + 1 + i;
    assert_ne!(buf[(x, row(0))].bg, selection, "row 0 is not selected");
    assert_eq!(buf[(x, row(1))].bg, selection, "row 1 is selected");
    assert_eq!(buf[(c.right() - 2, row(1))].bg, selection, "whole row");
    assert_ne!(buf[(x, row(2))].bg, selection, "row 2 is not selected");
    assert_ne!(buf[(c.x, row(1))].bg, selection, "border untouched");

    // Sem foco no painel, nenhuma linha é destacada.
    app.focus = Panel::Graph;
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer();
    assert_ne!(buf[(x, row(1))].bg, selection);
}
