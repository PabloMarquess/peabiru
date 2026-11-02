//! Snapshots do Tab Trace (ARCHITECTURE §6.6): o estado de
//! `pb tui --render-once` (grafo demo e eventos na base fixa, trace
//! concluído, relógio fixo) renderizado a 156×50 e 100×30 e comparado com
//! `tests/snapshots/trace_<WxH>.txt`. Em divergência, o teste falha com um
//! diff por linha. Para regenerar os arquivos:
//!
//! ```text
//! cargo run -q -p peabiru-cli -- tui --render-once --size 156x50 > crates/peabiru-tui/tests/snapshots/trace_156x50.txt
//! cargo run -q -p peabiru-cli -- tui --render-once --size 100x30 > crates/peabiru-tui/tests/snapshots/trace_100x30.txt
//! ```
//!
//! ou `PEABIRU_UPDATE_SNAPSHOTS=1 cargo test -p peabiru-tui --test snapshot`.
//!
//! Os findings das regras Lua não entram: o Tab Trace não os desenha e
//! `peabiru-tui` não depende de `peabiru-lua` (ARCHITECTURE §1).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use peabiru_discovery::demo::{self, DemoSensor};
use peabiru_tui::snapshot::{self, render_to_string};
use peabiru_tui::{App, Theme};

/// Variável de ambiente que reescreve os arquivos em vez de comparar.
const UPDATE_ENV: &str = "PEABIRU_UPDATE_SNAPSHOTS";
/// Máximo de linhas divergentes listadas na mensagem de falha.
const MAX_DIFF_LINES: usize = 60;

/// O mesmo estado de `pb tui --render-once --size WxH`.
fn render_once(width: u16, height: u16) -> String {
    let base = snapshot::fixed_base();
    let mut app = App::new(
        demo::graph_from(base),
        demo::default_request(),
        demo::environment(),
        Theme::default_dark(),
    );
    let mut sensor = DemoSensor::with_base(Duration::ZERO, base);
    snapshot::prepare(&mut app, &mut sensor, snapshot::fixed_clock());
    render_to_string(&app, width, height, false)
}

fn snapshot_path(width: u16, height: u16) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
        .join(format!("trace_{width}x{height}.txt"))
}

fn check(width: u16, height: u16) {
    let path = snapshot_path(width, height);
    let actual = render_once(width, height);
    if std::env::var_os(UPDATE_ENV).is_some() {
        std::fs::write(&path, &actual)
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e} (set {UPDATE_ENV}=1 to create it)",
            path.display()
        )
    });
    let frame_lines = actual.lines().count();
    assert_eq!(
        frame_lines,
        usize::from(height),
        "render has {frame_lines} lines"
    );
    for line in actual.lines() {
        assert!(
            unicode_width::UnicodeWidthStr::width(line) <= usize::from(width),
            "line wider than {width}: {line:?}"
        );
    }
    if actual != expected {
        panic!(
            "snapshot {} differs from the render ({UPDATE_ENV}=1 rewrites it):\n{}",
            path.display(),
            diff(&expected, &actual)
        );
    }
}

/// Diff simples por linha: para cada linha diferente, o número (1-based), a
/// esperada (`-`) e a obtida (`+`).
fn diff(expected: &str, actual: &str) -> String {
    let exp: Vec<&str> = expected.lines().collect();
    let act: Vec<&str> = actual.lines().collect();
    let mut out = String::new();
    let _ = writeln!(out, "--- expected (file, {} lines)", exp.len());
    let _ = writeln!(out, "+++ actual (render, {} lines)", act.len());
    let mut shown = 0;
    for i in 0..exp.len().max(act.len()) {
        let (e, a) = (exp.get(i).copied(), act.get(i).copied());
        if e == a {
            continue;
        }
        if shown == MAX_DIFF_LINES {
            let _ = writeln!(out, "... (more differences omitted)");
            break;
        }
        shown += 1;
        let _ = writeln!(out, "@@ line {}", i + 1);
        if let Some(e) = e {
            let _ = writeln!(out, "-{e}");
        }
        if let Some(a) = a {
            let _ = writeln!(out, "+{a}");
        }
    }
    out
}

#[test]
fn trace_tab_matches_reference_snapshot_156x50() {
    check(156, 50);
}

#[test]
fn trace_tab_matches_minimum_snapshot_100x30() {
    check(100, 30);
}

#[test]
fn diff_reports_line_numbers_and_both_lines() {
    let d = diff("a\nb\nc\n", "a\nX\nc\nd\n");
    assert!(d.contains("@@ line 2\n-b\n+X\n"), "{d}");
    assert!(d.contains("@@ line 4\n+d\n"), "{d}");
    assert!(!d.contains("@@ line 1\n"), "{d}");
    assert!(!d.contains("@@ line 3\n"), "{d}");
}
