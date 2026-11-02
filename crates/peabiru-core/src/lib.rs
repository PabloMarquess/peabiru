//! `peabiru-core`: modelo normalizado, grafo temporal, evidência, confiança,
//! eventos e findings. É a base de todos os outros crates do workspace e não
//! conhece nenhum provider específico (AWS, Kubernetes, ...) nem UI.
//!
//! Ver `ARCHITECTURE.md` §2 para o contrato público.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod diagnostics;
pub mod events;
pub mod evidence;
pub mod graph;
pub mod model;
pub mod status;

pub use diagnostics::*;
pub use events::*;
pub use evidence::*;
pub use graph::*;
pub use model::*;
pub use status::*;
