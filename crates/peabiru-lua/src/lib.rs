//! `peabiru-lua`: o runtime Lua 5.4 do Peabiru. Lua fica **fora do datapath**
//! (VISION §22): entra só nos diagnósticos (`RuleEngine`, regras que produzem
//! `Finding`s sobre um `TraceResult`) e na customização da UI (`Config`,
//! tema + lista de arquivos de regras lidos de `config.lua`).
//!
//! Ver `ARCHITECTURE.md` §5 para o contrato público e `rules/default.lua`
//! para a API disponível dentro de um arquivo de regras.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod config;
pub mod rules;

pub use config::{default_config_path, load_config, parse_config, Config};
pub use rules::RuleEngine;
