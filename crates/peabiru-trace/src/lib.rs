//! `peabiru-trace`: o trace engine do Peabiru. Dado o `Graph` global e um
//! `TraceRequest`, encontra o caminho principal entre origem e destino
//! (Dijkstra ponderado por confiança), coleta as branches (vizinhos fora do
//! caminho), as correlações relevantes e calcula a confiança fim a fim.
//!
//! Ver `ARCHITECTURE.md` §3 para o contrato público.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod confidence;
pub mod correlation;
pub mod engine;
pub mod error;
pub mod path;
pub mod request;
pub mod result;

pub use confidence::{edge_confidence, path_confidence};
pub use correlation::{collect_correlations, correlation_basis};
pub use engine::TraceEngine;
pub use error::TraceError;
pub use path::{PathStep, TraceTarget};
pub use request::TraceRequest;
pub use result::{Branch, Correlation, Direction, Hop, Summary, TraceResult, TraceStatus};
