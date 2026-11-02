//! Correlações relevantes para um trace: arestas `Relation::Correlated` cujos
//! dois extremos estão no caminho principal ou nas branches.

use std::collections::HashSet;

use peabiru_core::{Edge, EdgeId, Evidence, Graph, Relation};

use crate::result::{Branch, Correlation, Hop};

/// Arestas `Correlated` (em ordem de inserção) com `from` e `to` entre os
/// hops ou as branches.
pub fn correlation_edges<'a>(
    graph: &'a Graph,
    hops: &[Hop],
    branches: &[Branch],
) -> Vec<(EdgeId, &'a Edge)> {
    let members: HashSet<&str> = hops
        .iter()
        .map(|h| h.entity.as_str())
        .chain(branches.iter().map(|b| b.entity.as_str()))
        .collect();
    graph
        .edges()
        .filter(|(_, e)| {
            e.relation == Relation::Correlated
                && members.contains(e.from.as_str())
                && members.contains(e.to.as_str())
        })
        .collect()
}

/// `score = edge.confidence()`, `basis = correlation_basis(edge)`. Nomes vêm
/// das entidades (id se ausente).
pub fn collect_correlations(graph: &Graph, hops: &[Hop], branches: &[Branch]) -> Vec<Correlation> {
    correlation_edges(graph, hops, branches)
        .into_iter()
        .map(|(_, e)| Correlation {
            a: e.from.clone(),
            a_name: name_of(graph, &e.from),
            b: e.to.clone(),
            b_name: name_of(graph, &e.to),
            score: e.confidence(),
            basis: correlation_basis(e),
        })
        .collect()
}

/// Chave de metadata que uma aresta `Correlated` pode usar para nomear a base
/// da correlação (`"VPC Flow Logs + eBPF 5-tuple"`).
pub const BASIS_META: &str = "basis";

/// Texto do `basis` de uma aresta `Correlated`: `metadata["basis"]` da melhor
/// evidência (ou, na falta, da primeira evidência que o tenha); senão as
/// fontes distintas das evidências unidas por `" + "` (`""` sem evidência).
/// Uma correlação junta fontes: a aresta guarda as evidências reais e este
/// texto é só o rótulo exibido.
pub fn correlation_basis(edge: &Edge) -> String {
    let from_meta = |e: &Evidence| e.metadata.get(BASIS_META).cloned();
    if let Some(basis) = edge.best_evidence().and_then(from_meta) {
        return basis;
    }
    if let Some(basis) = edge.evidence.iter().find_map(from_meta) {
        return basis;
    }
    let mut sources: Vec<&str> = Vec::new();
    for e in &edge.evidence {
        if !sources.contains(&e.source.as_str()) {
            sources.push(e.source.as_str());
        }
    }
    sources.join(" + ")
}

fn name_of(graph: &Graph, id: &str) -> String {
    graph
        .entity(id)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use peabiru_core::{
        Confidence, Entity, EntityKind, Evidence, EvidenceKind, Metrics, NodeStatus, Provider,
    };

    fn hop(index: usize, id: &str) -> Hop {
        Hop {
            index,
            entity: id.to_string(),
            name: id.to_string(),
            kind: EntityKind::Host,
            status: NodeStatus::Observed,
            address: String::new(),
            edge_in: None,
            edge_status: NodeStatus::Observed,
            edge_source: None,
            evidence_kind: None,
            sources: 0,
            confidence: Confidence(100),
            metrics: Metrics::default(),
        }
    }

    #[test]
    fn only_correlated_edges_between_trace_members() {
        let mut g = Graph::new();
        for id in ["a", "b", "c", "z"] {
            g.upsert_entity(Entity::new(
                id,
                format!("name-{id}"),
                EntityKind::Host,
                Provider::Demo,
            ));
        }
        g.add_edge(Edge::new("a", "b", Relation::Connects));
        g.add_edge(
            Edge::new("a", "c", Relation::Correlated).with_evidence(Evidence::new(
                EvidenceKind::Observed,
                "timing correlation",
                Provider::Demo,
                Confidence(73),
            )),
        );
        g.add_edge(Edge::new("a", "z", Relation::Correlated));
        g.add_edge(Edge::new("b", "a", Relation::Correlated));
        let hops = vec![hop(0, "a"), hop(1, "b")];
        let branches = vec![Branch {
            anchor: 1,
            entity: "c".into(),
            name: "name-c".into(),
            kind: EntityKind::Cache,
            status: NodeStatus::Inferred,
            address: String::new(),
            edge: EdgeId(0),
            edge_status: NodeStatus::Inferred,
            direction: Direction::Outgoing,
        }];
        let cs = collect_correlations(&g, &hops, &branches);
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0].a_name, "name-a");
        assert_eq!(cs[0].b, "c");
        assert_eq!(cs[0].score, Confidence(73));
        assert_eq!(cs[0].basis, "timing correlation");
        assert_eq!(cs[1].score, Confidence(0));
        assert_eq!(cs[1].basis, "");
    }

    #[test]
    fn basis_prefers_metadata_then_joins_distinct_sources() {
        let ev =
            |kind, source: &str, pct| Evidence::new(kind, source, Provider::Demo, Confidence(pct));
        // Sem metadata: fontes distintas unidas por " + " (dedup).
        let joined = Edge::new("a", "b", Relation::Correlated)
            .with_evidence(ev(EvidenceKind::LogConfirmed, "VPC Flow Logs", 90))
            .with_evidence(ev(EvidenceKind::Observed, "eBPF", 92))
            .with_evidence(ev(EvidenceKind::ActiveTest, "eBPF", 50));
        assert_eq!(correlation_basis(&joined), "VPC Flow Logs + eBPF");
        assert_eq!(joined.status(), NodeStatus::Correlated);
        assert_eq!(joined.confidence(), Confidence(92));

        // metadata["basis"] na melhor evidência vence.
        let tagged = Edge::new("a", "b", Relation::Correlated)
            .with_evidence(ev(EvidenceKind::LogConfirmed, "VPC Flow Logs", 90))
            .with_evidence(
                ev(EvidenceKind::Observed, "eBPF", 92).with_meta(BASIS_META, "flow log + 5-tuple"),
            );
        assert_eq!(correlation_basis(&tagged), "flow log + 5-tuple");

        // metadata só numa evidência que não é a melhor: ainda assim vale.
        let secondary = Edge::new("a", "b", Relation::Correlated)
            .with_evidence(ev(EvidenceKind::Observed, "eBPF", 92))
            .with_evidence(
                ev(EvidenceKind::LogConfirmed, "VPC Flow Logs", 60).with_meta(BASIS_META, "tagged"),
            );
        assert_eq!(correlation_basis(&secondary), "tagged");

        assert_eq!(
            correlation_basis(&Edge::new("a", "b", Relation::Correlated)),
            ""
        );
    }

    use crate::result::Direction;
}
