//! `peabiru`: o binário do Peabiru (ARCHITECTURE §7).
//!
//! ```text
//! peabiru                       # = peabiru tui
//! peabiru tui [--provider demo] [--render-once --size WxH [--ansi]]
//! peabiru trace --from A --to B [--port 443] [--protocol HTTPS] [--json]
//! peabiru graph [--json]
//! peabiru rules check FILE...
//! ```
//!
//! Códigos de saída: 0 ok; 1 erro de execução; 2 erro de argumento (clap);
//! 3 trace falhou (`TraceError`).

#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod args;
mod bootstrap;
mod commands;
mod table;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = args::Cli::parse();
    match commands::run(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}
