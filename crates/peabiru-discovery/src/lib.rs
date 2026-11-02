//! `peabiru-discovery`: os traits `Provider` (quem popula o grafo com fatos
//! topológicos) e `Sensor` (quem emite observações), o registro de providers
//! e o provider `demo`, que reproduz a fixture dos mockups.
//!
//! Ver `ARCHITECTURE.md` §4 para o contrato público.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod demo;

use peabiru_core::{Event, Graph};

/// Fonte de descoberta (AWS, Kubernetes, demo, ...): escreve entidades e
/// arestas no `Graph`. Discovery fornece fatos, não decide o caminho (VISION §29).
pub trait Provider {
    /// `"demo"`, `"aws"`, `"kubernetes"`.
    fn name(&self) -> &'static str;
    /// Provider do core correspondente.
    fn provider(&self) -> peabiru_core::Provider;
    /// Popula (ou atualiza) o grafo e relata o que foi emitido.
    fn discover(&self, graph: &mut Graph) -> anyhow::Result<DiscoveryReport>;
}

/// Resultado de um `discover`: quantas entidades e arestas o provider emitiu
/// (via `upsert_entity`/`add_edge`; entidades já existentes contam) e avisos.
#[derive(Clone, Debug, Default)]
pub struct DiscoveryReport {
    pub entities: usize,
    pub edges: usize,
    pub warnings: Vec<String>,
}

/// Fonte de observações (eBPF, flow logs, ...): `poll` devolve os eventos
/// novos desde o último `poll`.
pub trait Sensor {
    fn name(&self) -> &'static str;
    fn poll(&mut self) -> Vec<Event>;
}

/// Providers disponíveis. Por enquanto só o `demo`.
pub fn registry() -> Vec<Box<dyn Provider>> {
    vec![Box::new(demo::DemoProvider)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_demo() {
        let r = registry();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name(), "demo");
        assert_eq!(r[0].provider(), peabiru_core::Provider::Demo);
        let mut g = Graph::new();
        let report = r[0].discover(&mut g).unwrap();
        assert_eq!(report.entities, g.entity_count());
        assert_eq!(report.edges, g.edge_count());
        assert!(report.warnings.is_empty());
    }
}
