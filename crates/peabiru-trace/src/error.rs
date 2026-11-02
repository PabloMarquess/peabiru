//! Erros do trace engine.

use thiserror::Error;

/// Falhas possíveis de `TraceEngine::trace`.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum TraceError {
    /// O selector de origem não resolveu para nenhuma entidade do grafo.
    #[error("source not found: {0}")]
    SourceNotFound(String),
    /// O selector de destino não resolveu para nenhuma entidade do grafo.
    #[error("target not found: {0}")]
    TargetNotFound(String),
    /// Origem e destino existem, mas não há caminho por arestas de saída.
    #[error("no path from {from} to {to}")]
    NoPath { from: String, to: String },
}
