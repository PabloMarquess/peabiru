//! Smoke test: o layout de referência renderiza e as teclas básicas funcionam.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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

#[test]
fn renders_reference_layout() {
    let mut app = app();
    app.run_trace();
    assert!(app.trace.is_some());

    let out = render_to_string(&app, 156, 50, false);
    assert_eq!(out.lines().count(), 50);
    assert!(out.lines().all(|l| l.chars().count() <= 156));
    for needle in [
        "TRACE GRAPH",
        "NODE DETAILS",
        "PEABIRU",
        "TRACE TARGET",
        "TRACE SUMMARY",
        "LEGEND",
        "CORRELATIONS",
        "EVENTS",
        "KEYMAP",
    ] {
        assert!(out.contains(needle), "missing {needle:?} in:\n{out}");
    }

    let ansi = render_to_string(&app, 156, 50, true);
    assert!(ansi.contains("\x1b[0;"));
    assert!(ansi.contains("38;2;"));
}

#[test]
fn renders_minimum_and_too_small() {
    let app = app();
    let out = render_to_string(&app, 100, 30, false);
    assert!(out.contains("TRACE GRAPH"), "{out}");
    let small = render_to_string(&app, 80, 24, false);
    assert!(small.contains("terminal too small"), "{small}");
    assert!(!small.contains("TRACE GRAPH"));
}

#[test]
fn keys_switch_tab_quit_and_help() {
    let mut app = app();
    app.on_key(key('2'));
    assert_eq!(app.tab, Tab::Topology);
    app.on_key(key('?'));
    assert!(app.help_open);
    app.on_key(key('?'));
    assert!(!app.help_open, "any key closes the help popup");
    app.on_key(key('?'));
    assert!(app.help_open);
    let out = render_to_string(&app, 156, 50, false);
    assert!(out.contains("HELP"), "{out}");
    app.on_key(key('q'));
    assert!(!app.should_quit, "q with a popup open only closes it");
    app.on_key(key('q'));
    assert!(app.should_quit);
}
