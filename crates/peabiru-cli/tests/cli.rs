//! Testes de integração dos binários `pb` e `peabiru` (ARCHITECTURE §7).

use std::process::{Command, Output};

/// Caminho inexistente: isola os testes do `config.lua` do usuário
/// (arquivo ausente → `Config::default()`).
const NO_CONFIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/no-such-config.lua");
const DEFAULT_RULES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../rules/default.lua");

fn run(bin: &str, args: &[&str]) -> Output {
    Command::new(bin)
        .args(["--config", NO_CONFIG])
        .args(args)
        .output()
        .expect("failed to run the CLI")
}

fn pb(args: &[&str]) -> Output {
    run(env!("CARGO_BIN_EXE_pb"), args)
}

fn peabiru(args: &[&str]) -> Output {
    run(env!("CARGO_BIN_EXE_peabiru"), args)
}

#[test]
fn peabiru_is_the_same_command_as_pb() {
    assert_eq!(
        stdout(&peabiru(&["--version"])),
        stdout(&pb(&["--version"]))
    );
    let rules = ["rules", "check", DEFAULT_RULES];
    assert_eq!(stdout(&peabiru(&rules)), stdout(&pb(&rules)));

    let short = stdout(&pb(&["--help"]));
    let full = stdout(&peabiru(&["--help"]));
    assert!(short.contains("Usage: pb [OPTIONS] [COMMAND]"), "{short}");
    assert!(
        full.contains("Usage: peabiru [OPTIONS] [COMMAND]"),
        "{full}"
    );
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn version_prints_name_and_version() {
    let out = pb(&["--version"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stdout(&out).trim(), "peabiru 0.1.0");
}

#[test]
fn trace_json_has_five_hops() {
    let out = pb(&[
        "trace",
        "--from",
        "web-frontend-7f9c8c",
        "--to",
        "api.prod.svc.cluster.local",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout is not valid JSON");
    let hops = value["hops"].as_array().expect("hops is an array");
    assert_eq!(hops.len(), 5);
    assert_eq!(value["request"]["to"], "api.prod.svc.cluster.local");
    assert_eq!(value["summary"]["confidence"], 92);
    let findings = value["findings"].as_array().expect("findings is an array");
    assert_eq!(findings.len(), 1, "default rules: tromboning on the demo");
    assert_eq!(findings[0]["rule"], "default.lua");
}

#[test]
fn trace_text_renders_ascii_path() {
    let out = pb(&[
        "trace",
        "--from",
        "web-frontend-7f9c8c",
        "--to",
        "api.prod.svc.cluster.local",
    ]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.starts_with("PEABIRU TRACE #"), "{text}");
    for needle in [
        "web-frontend-7f9c8c",
        "aws-alb",
        "app-svc",
        "Path confidence:     92%",
        "[warning] Possible traffic tromboning",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
}

#[test]
fn trace_unknown_source_exits_3() {
    let out = pb(&["trace", "--from", "nope", "--to", "x"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(stderr(&out).contains("source not found: nope"));
    assert!(stdout(&out).is_empty());
}

#[test]
fn trace_missing_args_exits_2() {
    let out = pb(&["trace", "--from", "a"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn tui_render_once_prints_reference_frame() {
    let out = pb(&["tui", "--render-once", "--size", "156x50"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let frame = stdout(&out);
    assert!(frame.contains("TRACE GRAPH"), "{frame}");
    assert!(frame.contains("aws-alb"), "{frame}");
    assert_eq!(frame.lines().count(), 50);
    assert!(frame.lines().all(|l| l.chars().count() <= 156));
    assert!(!frame.contains('\x1b'), "plain mode has no escapes");
}

#[test]
fn tui_render_once_ansi_and_no_bg() {
    let out = pb(&["tui", "--render-once", "--ansi"]);
    assert_eq!(out.status.code(), Some(0));
    let frame = stdout(&out);
    assert!(frame.contains("\x1b[0;"));
    assert!(frame.contains("48;2;11;14;20"), "default bg is painted");

    let out = pb(&["--no-bg", "tui", "--render-once", "--ansi"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        !stdout(&out).contains("48;2;11;14;20"),
        "--no-bg skips the bg"
    );
}

#[test]
fn tui_bad_size_and_provider_exit_2() {
    let out = pb(&["tui", "--render-once", "--size", "156"]);
    assert_eq!(out.status.code(), Some(2));
    let out = pb(&["tui", "--render-once", "--provider", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("unknown provider 'nope'"));
}

#[test]
fn graph_table_and_json() {
    let out = pb(&["graph"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.starts_with("NAME"), "{text}");
    assert!(text.contains("FROM"), "{text}");
    assert!(text.contains("aws-alb"), "{text}");
    assert!(text.contains("load_balancer"), "{text}");

    let out = pb(&["graph", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(value["entities"].as_object().map(|m| m.len()), Some(10));
    assert_eq!(value["edges"].as_array().map(|a| a.len()), Some(12));
}

#[test]
fn rules_check_default_rules() {
    let out = pb(&["rules", "check", DEFAULT_RULES]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("[WARNING] default.lua: Possible traffic tromboning"),
        "{text}"
    );
    // O arquivo é o default embutido: roda uma vez só.
    assert_eq!(text.matches("tromboning").count(), 1, "{text}");
    assert!(stderr(&out).contains("1 rule file(s)"), "{}", stderr(&out));
}

#[test]
fn rules_check_missing_file_exits_1() {
    let out = pb(&["rules", "check", "/no/such/rules.lua"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("cannot load rules file"));
}
