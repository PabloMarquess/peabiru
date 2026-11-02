//! Bootstrap comum aos subcomandos: `config.lua` → grafo (providers do
//! registro) → `RuleEngine` (default + config + `--rules`) → `Theme`.
//! Avisos não fatais vão para stderr com o prefixo `warning:`.

use anyhow::Context;
use peabiru_core::Graph;
use peabiru_discovery::registry;
use peabiru_lua::{Config, RuleEngine};
use peabiru_tui::Theme;

use crate::args::Cli;

/// Nomes dos providers registrados (`"demo"`, ...).
pub fn provider_names() -> Vec<&'static str> {
    registry().iter().map(|p| p.name()).collect()
}

/// `config.lua` de `--config` ou do caminho padrão; ausente → `Config::default()`.
pub fn load_config(cli: &Cli) -> anyhow::Result<Config> {
    peabiru_lua::load_config(cli.config.as_deref()).context("cannot load config.lua")
}

/// Roda `discover` de cada provider do registro (só `provider`, se dado) num
/// grafo novo. Avisos e falhas de provider vão para stderr; a falha de um
/// provider não impede os outros.
pub fn build_graph(provider: Option<&str>) -> Graph {
    let mut graph = Graph::new();
    for p in registry() {
        if provider.is_some_and(|name| name != p.name()) {
            continue;
        }
        match p.discover(&mut graph) {
            Ok(report) => {
                for w in report.warnings {
                    eprintln!("warning: provider {}: {w}", p.name());
                }
            }
            Err(err) => eprintln!("warning: provider {} failed: {err:#}", p.name()),
        }
    }
    graph
}

/// `RuleEngine` com `rules/default.lua`, depois `config.rules` (falha vira
/// aviso) e por fim `--rules` (falha é erro: o usuário pediu explicitamente).
pub fn build_engine(config: &Config, cli: &Cli) -> anyhow::Result<RuleEngine> {
    let mut engine = RuleEngine::new().context("cannot start the Lua rule engine")?;
    engine
        .load_default()
        .context("cannot load the default rules")?;
    for path in &config.rules {
        if let Err(err) = engine.load_file(path) {
            eprintln!("warning: config.lua rules: {err:#}");
        }
    }
    for path in &cli.rules {
        engine
            .load_file(path)
            .with_context(|| format!("cannot load rules file {}", path.display()))?;
    }
    Ok(engine)
}

/// `Theme::default_dark()` + `config.theme`; chaves desconhecidas e cores
/// inválidas viram avisos. `paint_bg = !--no-bg`.
pub fn build_theme(config: &Config, cli: &Cli) -> Theme {
    let mut theme = Theme::default_dark();
    for rejected in theme.apply_overrides(&config.theme) {
        if config.theme.contains_key(&rejected) {
            eprintln!(
                "warning: config.lua theme: unknown key '{rejected}' (known: {})",
                Theme::names().join(", ")
            );
        } else {
            eprintln!("warning: config.lua theme: {rejected}");
        }
    }
    theme.paint_bg = !cli.no_bg;
    theme
}
