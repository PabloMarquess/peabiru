//! Popups (help, details) e tabs 2–5 renderizam o conteúdo esperado nos
//! dois tamanhos de referência (156×50 e 100×30).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use peabiru_core::{Finding, Severity};
use peabiru_discovery::demo;
use peabiru_tui::app::Tab;
use peabiru_tui::{render_to_string, App, Theme};

fn app() -> App {
    App::new(
        demo::graph(),
        demo::default_request(),
        demo::environment(),
        Theme::default_dark(),
    )
}

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// Renderiza e garante que a moldura cabe: N linhas, nenhuma mais larga que `w`.
fn render(app: &App, w: u16, h: u16) -> String {
    let out = render_to_string(app, w, h, false);
    assert_eq!(out.lines().count(), usize::from(h));
    assert!(
        out.lines().all(|l| l.chars().count() <= usize::from(w)),
        "{out}"
    );
    out
}

fn assert_contains(out: &str, needles: &[&str]) {
    for needle in needles {
        assert!(out.contains(needle), "missing {needle:?} in:\n{out}");
    }
}

#[test]
fn help_popup_lists_keys() {
    let mut app = app();
    app.on_key(key('?'));
    assert!(app.help_open);
    for (w, h) in [(156, 50), (100, 30)] {
        let out = render(&app, w, h);
        assert_contains(
            &out,
            &[
                "HELP",
                "press any key",
                "switch tab",
                "run trace",
                "yolo mode",
                "TRACE TARGET → TRACE GRAPH",
            ],
        );
    }
}

#[test]
fn details_popup_shows_selected_node() {
    let mut app = app();
    app.run_trace();
    app.on_key(key('d'));
    assert!(app.details_open);
    let out = render(&app, 156, 50);
    assert_contains(
        &out,
        &[
            "NODE DETAILS · web-frontend-7f9c8c",
            "EVIDENCE",
            "ADDRESSES",
            "ATTRIBUTES",
            "EDGES OUT",
            "k8s://prod/pod/web-frontend-7f9c8c",
            "kube-proxy",
            "esc",
        ],
    );

    // aws-alb (hop 3): duas evidências, arestas nos dois sentidos e métricas.
    app.selected_node = 3;
    let out = render(&app, 100, 30);
    assert_contains(
        &out,
        &[
            "NODE DETAILS · aws-alb",
            "LOG_CONFIRMED",
            "VPC Flow Logs",
            "EDGES IN (1)",
            "EDGES OUT (2)",
            "rtt 1.103ms",
            "AS16509",
        ],
    );
}

#[test]
fn details_popup_without_selection() {
    let mut app = app();
    app.details_open = true;
    let out = render(&app, 156, 50);
    assert_contains(&out, &["NODE DETAILS", "no node selected"]);
}

#[test]
fn topology_tab_lists_entities_and_edges() {
    let mut app = app();
    app.on_key(key('2'));
    assert_eq!(app.tab, Tab::Topology);
    for (w, h) in [(156, 50), (100, 30)] {
        let out = render(&app, w, h);
        assert_contains(
            &out,
            &[
                "ENTITIES (10)",
                "EDGES (12)",
                "aws-alb",
                "RELATION",
                "NAME",
                "SOURCES",
                "CONF%",
                "load_balancer",
            ],
        );
    }
    // A linha selecionada segue `selected_node`; a última entidade continua visível.
    app.on_key(key('G'));
    let out = render(&app, 100, 30);
    assert!(out.contains("api.prod.svc.cluster.local"), "{out}");
}

#[test]
fn correlations_tab_table_and_detail() {
    let mut app = app();
    app.on_key(key('3'));
    let out = render(&app, 156, 50);
    assert_contains(
        &out,
        &[
            "CORRELATIONS (0)",
            "run a trace first",
            "no correlation selected",
        ],
    );

    app.run_trace();
    for (w, h) in [(156, 50), (100, 30)] {
        let out = render(&app, w, h);
        assert_contains(
            &out,
            &[
                "CORRELATIONS (3)",
                "BASIS",
                "92",
                "SCORE",
                "VPC Flow Logs + eBPF 5-tuple",
                "CORRELATION",
                "app-svc",
            ],
        );
    }
    app.on_key(key('j'));
    let out = render(&app, 156, 50);
    assert!(out.contains("cgroup + flow log"), "{out}");
}

#[test]
fn observability_tab_findings_metrics_sources() {
    let mut app = app();
    app.on_key(key('4'));
    let out = render(&app, 156, 50);
    assert_contains(
        &out,
        &[
            "FINDINGS",
            "PATH METRICS",
            "EVENT SOURCES",
            "run a trace first",
        ],
    );

    app.run_trace();
    app.findings = vec![
        Finding::new(
            Severity::Warning,
            "default.lua",
            "Possible traffic tromboning",
        )
        .with_subject(demo::NAT_GW),
        Finding::new(Severity::Critical, "custom.lua", "Kernel drops on path"),
        Finding::new(Severity::Info, "custom.lua", "Low path confidence"),
    ];
    for (w, h) in [(156, 50), (100, 30)] {
        let out = render(&app, w, h);
        assert_contains(
            &out,
            &[
                "FINDINGS (3)",
                "[WARNING] Possible traffic tromboning",
                "[CRITICAL]",
                "[INFO]",
                "MAX RTT",
                "1.217 ms",
                "CONFIDENCE",
                "92%",
                "eBPF",
                "flow_log",
                "total",
            ],
        );
    }
    // Regra e sujeito só cabem inteiros no painel largo; a 100 colunas a
    // linha é truncada com `…`.
    let out = render(&app, 156, 50);
    assert!(out.contains("default.lua @ nat-gw"), "{out}");
}

#[test]
fn settings_tab_theme_config_keymap() {
    let mut app = app();
    app.on_key(key('5'));
    for (w, h) in [(156, 50), (100, 30)] {
        let out = render(&app, w, h);
        assert_contains(
            &out,
            &[
                "THEME",
                "observed",
                "#5fd75f",
                "border_focus",
                "CONFIG",
                "(see --config)",
                &format!("{w}x{h}"),
                "KEYMAP",
                "switch tab",
                "first / last",
                "yolo off",
            ],
        );
    }
}
