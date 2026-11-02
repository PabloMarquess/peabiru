//! Painéis da coluna esquerda (TRACE TARGET, TRACE SUMMARY, LEGEND) no
//! layout de referência 156×50.

use peabiru_discovery::demo;
use peabiru_tui::app::{Panel, RunState};
use peabiru_tui::ui::trace_summary::NONE;
use peabiru_tui::{render_to_string, App, Theme};

fn app() -> App {
    App::new(
        demo::graph(),
        demo::default_request(),
        demo::environment(),
        Theme::default_dark(),
    )
}

/// Linhas da coluna esquerda (até a borda direita do primeiro painel).
fn left_column(out: &str) -> Vec<String> {
    out.lines()
        .map(|l| {
            let end = l.char_indices().nth(41).map_or(l.len(), |(i, _)| i);
            l[..end].to_string()
        })
        .collect()
}

#[test]
fn idle_state_shows_dashes() {
    let app = app();
    let out = render_to_string(&app, 156, 50, false);
    assert!(out.contains("Idle"), "{out}");
    assert!(out.contains("[R] Run Trace"), "{out}");
    assert!(!out.contains("Completed"), "{out}");
    let lines = left_column(&out);
    let duration = lines
        .iter()
        .find(|l| l.contains("DURATION"))
        .expect("DURATION row");
    let value = duration
        .split_once("DURATION")
        .map(|(_, rest)| rest.trim_matches(|c: char| c.is_whitespace() || c == '│'))
        .expect("DURATION value");
    assert_eq!(value, NONE, "{duration}");
}

#[test]
fn running_state_shows_spinner() {
    let mut app = app();
    app.run_trace();
    assert!(app.run.is_running());
    let out = render_to_string(&app, 156, 50, false);
    assert!(out.contains("tracing…"), "{out}");
    assert!(out.contains("Running"), "{out}");
    assert!(!out.contains("[R] Run Trace"), "{out}");
}

#[test]
fn completed_trace_renders_left_column() {
    let mut app = app();
    app.run_trace();
    app.run = RunState::Done;
    let out = render_to_string(&app, 156, 50, false);
    assert_eq!(out.lines().count(), 50);
    for needle in [
        "PORT: 443 (HTTPS)",
        "FROM: web-frontend-7f9c8c",
        "[R] Run Trace",
        "STATUS",
        "Completed",
        "DURATION",
        "2.843s",
        "HOPS",
        "OBSERVED",
        "● 5 (100%)",
        "INFERRED",
        "○ 0 (0%)",
        "CORRELATIONS",
        "CONFIDENCE",
        "92%",
        "■■■■■■■■■■■■",
        "observed node",
        "inferred node",
        "correlated node",
        "probe point",
        "── observed path",
        "╌╌ inferred path",
        "┄┄ possible path",
        "→ direction",
    ] {
        assert!(out.contains(needle), "missing {needle:?} in:\n{out}");
    }

    // A coluna esquerda nunca invade a borda: cada linha de conteúdo dos
    // painéis começa e termina com a borda vertical.
    let lines = left_column(&out);
    for (i, l) in lines.iter().enumerate().take(41).skip(3) {
        if l.starts_with('│') {
            assert!(l.trim_end().ends_with('│'), "line {i} overflows: {l:?}");
        }
    }
}

#[test]
fn failed_trace_shows_error() {
    let mut app = app();
    app.request.to = "does-not-exist".into();
    app.run_trace();
    let out = render_to_string(&app, 156, 50, false);
    assert!(out.contains("Failed"), "{out}");
    assert!(out.contains("target not found"), "{out}");
}

#[test]
fn focus_highlights_trace_target_border() {
    let mut app = app();
    app.focus = Panel::TraceTarget;
    let ansi = render_to_string(&app, 156, 50, true);
    let focus = "38;2;229;180;46";
    let title_line = ansi
        .lines()
        .find(|l| l.contains("TRACE TARGET"))
        .expect("title line");
    assert!(title_line.contains(focus), "{title_line}");
}

#[test]
fn minimum_size_does_not_overflow() {
    let mut app = app();
    app.run_trace();
    app.run = RunState::Done;
    let out = render_to_string(&app, 100, 30, false);
    assert!(out.contains("TRACE TARGET"), "{out}");
    assert!(out.contains("PORT: 443"), "{out}");
    assert!(out.lines().all(|l| l.chars().count() <= 100));
}
