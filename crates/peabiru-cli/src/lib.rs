//! A linha de comando do Peabiru (ARCHITECTURE §7), instalada como `pb` e,
//! com o nome completo, como `peabiru`. Os dois binários chamam [`main`].
//!
//! ```text
//! pb                       # = pb tui
//! pb tui [--provider demo] [--render-once --size WxH [--ansi]]
//! pb trace --from A --to B [--port 443] [--protocol HTTPS] [--json]
//! pb graph [--json]
//! pb rules check FILE...
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

/// Ponto de entrada comum de `pb` e `peabiru`. O nome mostrado no uso e na
/// ajuda é o que a pessoa digitou.
pub fn main() -> ExitCode {
    let cli = args::Cli::parse();
    match commands::run(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}
