//! Header, rodapé, EVENTS e KEYMAP no terminal de referência (156×50) e no
//! mínimo (100×30).

use std::time::Duration;

use peabiru_discovery::demo::{self, DemoSensor};
use peabiru_tui::app::Panel;
use peabiru_tui::{render_to_string, App, Theme};

const W: u16 = 156;
const H: u16 = 50;

fn app() -> App {
    App::new(
        demo::graph(),
        demo::default_request(),
        demo::environment(),
        Theme::default_dark(),
    )
}

fn assert_contains(out: &str, needles: &[&str]) {
    for needle in needles {
        assert!(out.contains(needle), "missing {needle:?} in:\n{out}");
    }
}

fn line(out: &str, idx: usize) -> String {
    out.lines().nth(idx).unwrap_or_default().to_string()
}

/// As 8 linhas do rodapé de painéis (EVENTS + KEYMAP): header 3 + main 38.
fn bottom_panels(out: &str) -> String {
    out.lines().skip(41).take(8).collect::<Vec<_>>().join("\n")
}

#[test]
fn chrome_at_reference_size() {
    let out = render_to_string(&app(), W, H, false);
    assert_eq!(out.lines().count(), usize::from(H));
    assert!(out.lines().all(|l| l.chars().count() <= usize::from(W)));
    assert_contains(
        &out,
        &[
            "PEABIRU",
            "NETWORK TRACING",
            "[1] Trace",
            "[2] Topology",
            "[3] Correlations",
            "[4] Observability",
            "[5] Settings",
            "? Help",
            "q Quit",
            "connected ●",
            "peabiru.local:443",
            "peabiru v0.1.0",
            "environment:",
            "prod-e1",
            "user:",
            "admin",
            "press ? for help",
            "EVENTS",
            "[p] pause",
            "waiting for events",
            "KEYMAP",
            "run trace",
            "next panel",
            "navigate",
            "select",
            "node details",
            "yolo mode",
            "pause events",
            "quit",
        ],
    );

    // Header: exatamente 3 linhas de blocos arredondados, conteúdo na do meio.
    let top = line(&out, 0);
    let mid = line(&out, 1);
    let bot = line(&out, 2);
    assert!(top.starts_with('╭') && top.ends_with('╮'), "{top}");
    assert!(bot.starts_with('╰') && bot.ends_with('╯'), "{bot}");
    assert_eq!(top.chars().count(), usize::from(W));
    assert_contains(
        &mid,
        &[
            "PEABIRU",
            "NETWORK TRACING",
            "[1] Trace",
            "q Quit",
            "connected ●",
            "peabiru.local:443",
        ],
    );
    assert!(!mid.contains("YOLO"));

    // Rodapé na última linha.
    let footer = line(&out, usize::from(H) - 1);
    assert!(footer.starts_with(" peabiru v0.1.0"), "{footer}");
    assert!(footer.ends_with("press ? for help"), "{footer}");
    assert!(
        footer.contains("environment: prod  │  cluster: prod-e1  │  user: admin"),
        "{footer}"
    );
}

#[test]
fn events_stream_auto_scrolls_and_pause_follows_selection() {
    let mut app = app();
    app.run_trace();
    let mut sensor = DemoSensor::new(Duration::ZERO);
    for _ in 0..3 {
        app.on_tick(&mut sensor);
    }
    assert_eq!(
        app.events.len(),
        7,
        "zero-interval demo sensor emits everything at once"
    );

    let out = render_to_string(&app, W, H, false);
    let panels = bottom_panels(&out);
    assert_contains(
        &panels,
        &[
            "EVENTS",
            "[p] pause",
            "eBPF",
            "connect",
            "kube-proxy:15001",
            "istio-ingressgateway:15001",
            "0.892ms",
            "↔",
            "92%",
            "(inferred)",
            "Kubernetes DNS",
        ],
    );
    assert!(
        !panels.contains("1.217ms"),
        "6 rows: the first event scrolled out:\n{panels}"
    );
    assert!(
        !panels.contains('…'),
        "no truncation at the reference width:\n{panels}"
    );
    assert!(panels.lines().all(|l| !l.contains("waiting for events")));

    app.toggle_pause();
    app.focus = Panel::Events;
    app.select_first();
    let out = render_to_string(&app, W, H, false);
    let panels = bottom_panels(&out);
    assert_contains(
        &panels,
        &["[p] resume", "1.217ms", "kube-proxy:15001", "eBPF"],
    );
    assert!(!panels.contains("[p] pause"));
    // Linha selecionada (a primeira) com fundo `selection` (#1c2330); a seguinte sem.
    let ansi = render_to_string(&app, W, H, true);
    let row = |i: usize| ansi.lines().nth(i).unwrap_or_default().to_string();
    assert!(row(42).contains("48;2;28;35;48"), "{}", row(42));
    assert!(!row(43).contains("48;2;28;35;48"), "{}", row(43));
    assert!(
        !panels.contains("Kubernetes DNS"),
        "window moved to the top:\n{panels}"
    );
}

#[test]
fn rerun_replays_the_timeline_from_the_first_event() {
    let mut app = app();
    let mut sensor = DemoSensor::new(Duration::ZERO);
    app.on_tick(&mut sensor);
    app.run_trace();
    assert!(app.events.is_empty());
    app.on_tick(&mut sensor);
    let panels = bottom_panels(&render_to_string(&app, W, H, false));
    assert_contains(&panels, &["eBPF", "connect", "kube-proxy:15001", "1.217ms"]);
}

#[test]
fn yolo_indicator_in_header() {
    let mut app = app();
    app.toggle_yolo();
    assert!(app.yolo);
    let mid = line(&render_to_string(&app, W, H, false), 1);
    assert_contains(
        &mid,
        &[
            "YOLO",
            "connected ●",
            "peabiru.local:443",
            "[5] Settings",
            "q Quit",
        ],
    );
    assert!(mid.chars().count() <= usize::from(W));
}

#[test]
fn minimum_size_degrades_without_clipping() {
    let mut app = app();
    let out = render_to_string(&app, 100, 30, false);
    assert!(out.lines().all(|l| l.chars().count() <= 100));
    let mid = line(&out, 1);
    assert_contains(
        &mid,
        &["PEABIRU", "[1] Trace", "[5] Settings", "connected ●"],
    );
    assert!(mid.ends_with('│'), "{mid}");
    let footer = line(&out, 29);
    assert_contains(
        &footer,
        &[
            "peabiru v0.1.0",
            "environment:",
            "prod-e1",
            "press ? for help",
        ],
    );
    assert_contains(&out, &["EVENTS", "KEYMAP", "run trace"]);

    app.toggle_yolo();
    let mid = line(&render_to_string(&app, 100, 30, false), 1);
    assert_contains(&mid, &["YOLO", "[1] Trace", "[5] Settings"]);
}
