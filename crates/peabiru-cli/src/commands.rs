//! Implementação dos subcomandos (ARCHITECTURE §7).

use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Context;
use clap::error::ErrorKind;
use clap::CommandFactory;
use peabiru_core::{Finding, Graph, Severity};
use peabiru_discovery::demo::{self, DemoSensor};
use peabiru_lua::RuleEngine;
use peabiru_trace::{TraceEngine, TraceRequest, TraceResult};
use peabiru_tui::app::FindingsFn;
use peabiru_tui::{render_to_string, snapshot, App};

use crate::args::{Cli, Cmd, RulesCmd, TraceArgs, TuiArgs};
use crate::{bootstrap, table};

/// Código de saída quando o trace falha (`TraceError`).
pub const EXIT_TRACE_FAILED: u8 = 3;

/// Intervalo entre eventos do `DemoSensor` no modo interativo.
const DEMO_SENSOR_INTERVAL: Duration = Duration::from_millis(350);

/// Despacha o subcomando (sem subcomando = `tui` com os padrões).
pub fn run(mut cli: Cli) -> anyhow::Result<ExitCode> {
    let cmd = cli
        .cmd
        .take()
        .unwrap_or_else(|| Cmd::Tui(TuiArgs::default()));
    match cmd {
        Cmd::Tui(args) => tui(&cli, &args),
        Cmd::Trace(args) => trace(&cli, &args),
        Cmd::Graph { json } => graph(&cli, json),
        Cmd::Rules {
            cmd: RulesCmd::Check { files },
        } => rules_check(&files),
    }
}

// --- tui ----------------------------------------------------------------

fn tui(cli: &Cli, args: &TuiArgs) -> anyhow::Result<ExitCode> {
    let providers = bootstrap::provider_names();
    if !providers.contains(&args.provider.as_str()) {
        Cli::command()
            .error(
                ErrorKind::InvalidValue,
                format!(
                    "unknown provider '{}' (available: {})",
                    args.provider,
                    providers.join(", ")
                ),
            )
            .exit();
    }

    let config = bootstrap::load_config(cli)?;
    // `--render-once` é o frame de referência (snapshots): grafo demo com
    // evidências numa base fixa, para o LAST SEEN não depender do relógio.
    let graph = if args.render_once {
        demo::graph_from(snapshot::fixed_base())
    } else {
        bootstrap::build_graph(Some(&args.provider))
    };
    let engine = Rc::new(bootstrap::build_engine(&config, cli)?);
    let theme = bootstrap::build_theme(&config, cli);

    let mut app = App::new(graph, demo::default_request(), demo::environment(), theme);
    app.findings_fn = Some(FindingsFn(Rc::new(move |trace, graph| {
        evaluate_or_warn(&engine, trace, graph)
    })));

    if args.render_once {
        let mut sensor = DemoSensor::with_base(Duration::ZERO, snapshot::fixed_base());
        snapshot::prepare(&mut app, &mut sensor, snapshot::fixed_clock());
        let (w, h) = args.size;
        print!("{}", render_to_string(&app, w, h, args.ansi));
        return Ok(ExitCode::SUCCESS);
    }

    let sensor = DemoSensor::new(DEMO_SENSOR_INTERVAL);
    peabiru_tui::run(app, Box::new(sensor))?;
    Ok(ExitCode::SUCCESS)
}

/// Avalia as regras; uma falha do runtime vira um finding `warning` em vez
/// de derrubar a interface.
fn evaluate_or_warn(engine: &RuleEngine, trace: &TraceResult, graph: &Graph) -> Vec<Finding> {
    match engine.evaluate(trace, graph) {
        Ok(findings) => findings,
        Err(err) => vec![Finding::new(
            Severity::Warning,
            "rules",
            &format!("rule engine error: {err:#}"),
        )],
    }
}

// --- trace --------------------------------------------------------------

fn trace(cli: &Cli, args: &TraceArgs) -> anyhow::Result<ExitCode> {
    let config = bootstrap::load_config(cli)?;
    let graph = bootstrap::build_graph(None);
    let engine = bootstrap::build_engine(&config, cli)?;

    let request = TraceRequest::new(&args.from, &args.to, args.port, &args.protocol);
    let mut result = match TraceEngine::trace(&graph, &request) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("error: {err}");
            return Ok(ExitCode::from(EXIT_TRACE_FAILED));
        }
    };
    result.findings = engine
        .evaluate(&result, &graph)
        .context("rule evaluation failed")?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result).context("cannot serialize the trace")?
        );
    } else {
        let text = result.render_ascii();
        print!("{text}");
        if !text.ends_with('\n') {
            println!();
        }
    }
    Ok(ExitCode::SUCCESS)
}

// --- graph --------------------------------------------------------------

fn graph(cli: &Cli, json: bool) -> anyhow::Result<ExitCode> {
    // O grafo não usa o config, mas um config.lua quebrado falha em todo
    // subcomando da mesma forma.
    bootstrap::load_config(cli)?;
    let graph = bootstrap::build_graph(None);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&graph).context("cannot serialize the graph")?
        );
        return Ok(ExitCode::SUCCESS);
    }

    let entities: Vec<Vec<String>> = graph
        .entities()
        .map(|e| {
            vec![
                e.name.clone(),
                e.kind.label().to_string(),
                e.provider.label().to_string(),
                e.primary_address().map(|a| a.display()).unwrap_or_default(),
                e.status().label().to_string(),
            ]
        })
        .collect();
    let name_of = |id: &str| {
        graph
            .entity(id)
            .map_or_else(|| id.to_string(), |e| e.name.clone())
    };
    let edges: Vec<Vec<String>> = graph
        .edges()
        .map(|(_, e)| {
            vec![
                name_of(&e.from),
                name_of(&e.to),
                e.relation.label().to_string(),
                e.status().label().to_string(),
                e.confidence().to_string(),
            ]
        })
        .collect();

    print!(
        "{}",
        table::render(
            &["NAME", "KIND", "PROVIDER", "ADDRESS", "STATUS"],
            &entities
        )
    );
    println!();
    print!(
        "{}",
        table::render(&["FROM", "TO", "RELATION", "STATUS", "CONF"], &edges)
    );
    Ok(ExitCode::SUCCESS)
}

// --- rules --------------------------------------------------------------

fn rules_check(files: &[PathBuf]) -> anyhow::Result<ExitCode> {
    let mut engine = RuleEngine::new().context("cannot start the Lua rule engine")?;
    engine
        .load_default()
        .context("cannot load the default rules")?;
    for path in files {
        // O próprio `rules/default.lua` já está embutido: carregá-lo de novo
        // só duplicaria cada finding.
        if std::fs::read_to_string(path).is_ok_and(|src| src == peabiru_lua::rules::DEFAULT_RULES) {
            eprintln!(
                "note: {} is the embedded default rules file (already loaded)",
                path.display()
            );
            continue;
        }
        engine
            .load_file(path)
            .with_context(|| format!("cannot load rules file {}", path.display()))?;
    }

    let graph = demo::graph();
    let trace =
        TraceEngine::trace(&graph, &demo::default_request()).context("demo trace failed")?;
    let findings = engine
        .evaluate(&trace, &graph)
        .context("rule evaluation failed")?;

    eprintln!(
        "loaded {} rule file(s): {}; {} finding(s) on the demo trace",
        engine.rule_count(),
        engine.rule_names().join(", "),
        findings.len()
    );
    for f in &findings {
        println!(
            "[{}] {}: {}",
            f.severity.label().to_uppercase(),
            f.rule,
            f.message
        );
    }
    Ok(ExitCode::SUCCESS)
}
