//! Linha de comando com `clap` derive (ARCHITECTURE §7).

use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};

/// Tamanho padrão do frame de `--render-once` (terminal de referência).
pub const DEFAULT_SIZE: &str = "156x50";

/// Peabiru: end-to-end network path intelligence.
#[derive(Debug, Parser)]
#[command(
    name = "peabiru",
    version,
    about = "Peabiru: end-to-end network path intelligence",
    long_about = None,
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Option<Cmd>,

    /// Arquivo config.lua (padrão: $XDG_CONFIG_HOME/peabiru/config.lua)
    #[arg(long, value_name = "PATH", global = true)]
    pub config: Option<PathBuf>,

    /// Arquivo de regras Lua extra (repetível; carregado após as do config)
    #[arg(long = "rules", value_name = "PATH", global = true, action = ArgAction::Append)]
    pub rules: Vec<PathBuf>,

    /// Não pintar o fundo (mantém a cor do terminal)
    #[arg(long, global = true)]
    pub no_bg: bool,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Interface interativa (padrão quando não há subcomando)
    Tui(TuiArgs),
    /// Traça o caminho entre origem e destino e imprime o resultado
    Trace(TraceArgs),
    /// Dump do grafo global (tabela ou JSON)
    Graph {
        /// Imprime o grafo em JSON
        #[arg(long)]
        json: bool,
    },
    /// Regras de diagnóstico (Lua)
    Rules {
        #[command(subcommand)]
        cmd: RulesCmd,
    },
}

#[derive(Debug, Clone, Args)]
pub struct TuiArgs {
    /// Provider de discovery que popula o grafo
    #[arg(long, default_value = "demo", value_name = "NAME")]
    pub provider: String,

    /// Renderiza um único frame em stdout e sai (não precisa de terminal)
    #[arg(long)]
    pub render_once: bool,

    /// Tamanho do frame de --render-once, em colunas x linhas
    #[arg(long, default_value = DEFAULT_SIZE, value_name = "WxH", value_parser = parse_size)]
    pub size: (u16, u16),

    /// Emite cores ANSI (truecolor) no frame de --render-once
    #[arg(long)]
    pub ansi: bool,
}

impl Default for TuiArgs {
    fn default() -> Self {
        Self {
            provider: "demo".to_string(),
            render_once: false,
            size: (156, 50),
            ansi: false,
        }
    }
}

#[derive(Debug, Clone, Args)]
pub struct TraceArgs {
    /// Origem: id, nome, ip, ip:porta ou hostname de uma entidade
    #[arg(long, value_name = "SELECTOR")]
    pub from: String,

    /// Destino: seletor ou FQDN (ex.: api.prod.svc.cluster.local)
    #[arg(long, value_name = "SELECTOR|FQDN")]
    pub to: String,

    /// Porta de destino
    #[arg(long, default_value_t = 443)]
    pub port: u16,

    /// Protocolo (só rótulo por enquanto)
    #[arg(long, default_value = "HTTPS")]
    pub protocol: String,

    /// Imprime o TraceResult em JSON em vez do texto
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Subcommand)]
pub enum RulesCmd {
    /// Carrega arquivos de regras (além das padrão) e roda contra o trace demo
    Check {
        /// Arquivos .lua de regras
        #[arg(required = true, value_name = "FILE")]
        files: Vec<PathBuf>,
    },
}

/// `"156x50"` → `(156, 50)`.
pub fn parse_size(s: &str) -> Result<(u16, u16), String> {
    let (w, h) = s
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("expected WxH (e.g. {DEFAULT_SIZE}), got '{s}'"))?;
    let dim = |v: &str, what: &str| {
        v.trim()
            .parse::<u16>()
            .map_err(|e| format!("invalid {what} '{v}': {e}"))
    };
    let (w, h) = (dim(w, "width")?, dim(h, "height")?);
    if w == 0 || h == 0 {
        return Err("width and height must be greater than zero".to_string());
    }
    Ok((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn size_parsing() {
        assert_eq!(parse_size("156x50"), Ok((156, 50)));
        assert_eq!(parse_size("100X30"), Ok((100, 30)));
        assert!(parse_size("156").is_err());
        assert!(parse_size("0x50").is_err());
        assert!(parse_size("ax50").is_err());
    }

    #[test]
    fn defaults_match_contract() {
        let cli = Cli::parse_from(["peabiru"]);
        assert!(cli.cmd.is_none());
        let cli = Cli::parse_from(["peabiru", "tui"]);
        match cli.cmd {
            Some(Cmd::Tui(t)) => {
                assert_eq!(t.provider, "demo");
                assert_eq!(t.size, (156, 50));
                assert!(!t.render_once);
                let d = TuiArgs::default();
                assert_eq!((d.provider, d.size), (t.provider, t.size));
            }
            other => panic!("unexpected {other:?}"),
        }
        // Globais valem antes ou depois do subcomando; repetidas no mesmo
        // nível acumulam (clap não mescla níveis diferentes).
        let cli = Cli::parse_from([
            "peabiru", "--rules", "a.lua", "trace", "--from", "a", "--to", "b",
        ]);
        assert_eq!(cli.rules, vec![PathBuf::from("a.lua")]);
        let cli = Cli::parse_from([
            "peabiru", "trace", "--from", "a", "--to", "b", "--rules", "a.lua", "--rules", "b.lua",
        ]);
        assert_eq!(cli.rules.len(), 2);
        match cli.cmd {
            Some(Cmd::Trace(t)) => {
                assert_eq!(t.port, 443);
                assert_eq!(t.protocol, "HTTPS");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
