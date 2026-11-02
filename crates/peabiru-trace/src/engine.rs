//! `TraceEngine`: orquestra resolução de selectors, path finding, branches,
//! correlações e resumo.

use std::sync::atomic::{AtomicU32, Ordering};

use chrono::Utc;
use peabiru_core::{observational_sources, Confidence, EdgeId, Graph, NodeStatus, Timestamp};

use crate::confidence::{edge_confidence, path_confidence};
use crate::correlation::{collect_correlations, correlation_edges};
use crate::error::TraceError;
use crate::path::{collect_branches, resolve_target, shortest_path};
use crate::request::TraceRequest;
use crate::result::{Branch, Hop, Summary, TraceResult, TraceStatus};

/// Contador de ids de trace (por processo, começa em 1).
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// O trace engine (sem estado; ids vêm de um contador por processo).
pub struct TraceEngine;

impl TraceEngine {
    /// Próximo id de trace (monotônico dentro do processo).
    pub fn next_id() -> u32 {
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    }

    /// Executa o trace: resolve `from` e `to` (`to` segue `ResolvesTo`, ver
    /// `path::resolve_target`), acha o caminho, coleta branches e correlações
    /// e calcula o resumo. `findings` sai vazio (é preenchido por `peabiru-lua`).
    pub fn trace(graph: &Graph, req: &TraceRequest) -> Result<TraceResult, TraceError> {
        let started_at = Utc::now();
        let source = graph
            .resolve(&req.from)
            .ok_or_else(|| TraceError::SourceNotFound(req.from.clone()))?;
        let target = resolve_target(graph, &req.to)
            .ok_or_else(|| TraceError::TargetNotFound(req.to.clone()))?;
        let no_path = || TraceError::NoPath {
            from: req.from.clone(),
            to: req.to.clone(),
        };
        let steps = shortest_path(graph, &source.id, &target.entity).ok_or_else(no_path)?;

        let mut hops = Vec::with_capacity(steps.len());
        for (index, step) in steps.iter().enumerate() {
            let ent = graph.entity(&step.entity).ok_or_else(no_path)?;
            let edge = step.edge_in.and_then(|eid| graph.edge(eid));
            let best = edge.and_then(|e| e.best_evidence());
            hops.push(Hop {
                index,
                entity: ent.id.clone(),
                name: ent.name.clone(),
                kind: ent.kind,
                status: ent.status(),
                address: ent
                    .primary_address()
                    .map(|a| a.display())
                    .unwrap_or_default(),
                edge_in: step.edge_in,
                edge_status: edge.map_or(NodeStatus::Observed, |e| e.status()),
                edge_source: best.map(|ev| ev.source.clone()),
                evidence_kind: best.map(|ev| ev.kind),
                sources: edge.map_or(0, |e| observational_sources(&e.evidence).len()),
                confidence: edge.map_or(Confidence::VERIFIED, |e| {
                    Confidence::from_ratio(edge_confidence(e))
                }),
                metrics: edge.map(|e| e.metrics.clone()).unwrap_or_default(),
            });
        }

        // Toda entidade atravessada ao desembrulhar `ResolvesTo` (a `selected`
        // e os aliases intermediários da cadeia CNAME) é resolução, não
        // topologia: nenhuma vira branch.
        let excluded: Vec<&str> = target
            .resolution
            .iter()
            .filter_map(|eid| graph.edge(*eid))
            .map(|e| e.from.as_str())
            .collect();
        let branches = collect_branches(graph, &hops, &excluded);
        let correlations = collect_correlations(graph, &hops, &branches);

        let observed = hops.iter().filter(|h| h.status.is_observed()).count();
        let summary = Summary {
            hops: hops.len(),
            observed,
            inferred: hops.len() - observed,
            correlations: correlations.len(),
            confidence: path_confidence(&hops),
            duration_ms: observation_window_ms(graph, &hops, &branches, &target.resolution),
        };

        Ok(TraceResult {
            id: Self::next_id(),
            status: TraceStatus::Completed,
            request: req.clone(),
            started_at,
            hops,
            branches,
            correlations,
            summary,
            findings: Vec::new(),
        })
    }
}

/// Janela de observação: diferença em ms entre a evidência mais antiga e a
/// mais recente entre as entidades dos hops, as arestas de entrada, as arestas
/// das branches, as correlações e as arestas `ResolvesTo` seguidas. 0 sem evidência.
fn observation_window_ms(
    graph: &Graph,
    hops: &[Hop],
    branches: &[Branch],
    resolution: &[EdgeId],
) -> u64 {
    let mut stamps: Vec<Timestamp> = Vec::new();
    for hop in hops {
        if let Some(ent) = graph.entity(&hop.entity) {
            stamps.extend(ent.evidence.iter().map(|e| e.timestamp));
        }
    }
    let edge_ids = hops
        .iter()
        .filter_map(|h| h.edge_in)
        .chain(branches.iter().map(|b| b.edge))
        .chain(
            correlation_edges(graph, hops, branches)
                .into_iter()
                .map(|(eid, _)| eid),
        )
        .chain(resolution.iter().copied());
    for eid in edge_ids {
        if let Some(edge) = graph.edge(eid) {
            stamps.extend(edge.evidence.iter().map(|e| e.timestamp));
        }
    }
    match (stamps.iter().min(), stamps.iter().max()) {
        (Some(min), Some(max)) => u64::try_from((*max - *min).num_milliseconds()).unwrap_or(0),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use peabiru_core::{
        Address, Edge, Entity, EntityKind, Evidence, EvidenceKind, Metrics, Provider, Relation,
    };
    use std::net::IpAddr;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn ev(kind: EvidenceKind, source: &str, pct: u8, at: Timestamp) -> Evidence {
        Evidence::new(kind, source, Provider::Demo, Confidence(pct)).at(at)
    }

    /// web → svc, `dns://api` ResolvesTo svc, svc → db (branch), web↔db correlados.
    fn fixture() -> (Graph, Timestamp) {
        let t0 = Utc::now();
        let mut g = Graph::new();
        g.upsert_entity(
            Entity::new(
                "k8s://pod/web",
                "web",
                EntityKind::Pod,
                Provider::Kubernetes,
            )
            .with_address(Address::socket(ip("10.42.1.15"), 51542))
            .with_evidence(ev(EvidenceKind::Observed, "eBPF", 95, t0)),
        );
        g.upsert_entity(
            Entity::new(
                "k8s://svc/app",
                "app-svc",
                EntityKind::Service,
                Provider::Kubernetes,
            )
            .with_address(Address::socket(ip("10.0.2.34"), 443))
            .with_evidence(ev(EvidenceKind::Observed, "eBPF", 95, t0)),
        );
        g.upsert_entity(
            Entity::new(
                "dns://api",
                "api.example",
                EntityKind::Endpoint,
                Provider::Kubernetes,
            )
            .with_address(Address::host("api.example")),
        );
        g.upsert_entity(
            Entity::new("aws://rds/db", "db", EntityKind::Database, Provider::Aws)
                .with_address(Address::socket(ip("10.0.4.12"), 5432)),
        );
        g.upsert_entity(Entity::new(
            "ip://1.1.1.1",
            "lonely",
            EntityKind::Ip,
            Provider::Unknown,
        ));
        g.add_edge(
            Edge::new("k8s://pod/web", "k8s://svc/app", Relation::Connects)
                .with_evidence(ev(EvidenceKind::Observed, "eBPF", 92, t0))
                .with_metrics(Metrics {
                    rtt_ms: Some(1.5),
                    retransmissions: 2,
                    drops: 0,
                    rst: 0,
                }),
        );
        g.add_edge(
            Edge::new("dns://api", "k8s://svc/app", Relation::ResolvesTo).with_evidence(ev(
                EvidenceKind::Discovered,
                "Kubernetes DNS",
                90,
                t0 + Duration::milliseconds(1500),
            )),
        );
        g.add_edge(
            Edge::new("k8s://svc/app", "aws://rds/db", Relation::Connects).with_evidence(ev(
                EvidenceKind::Inferred,
                "route inference",
                60,
                t0 + Duration::milliseconds(700),
            )),
        );
        g.add_edge(
            Edge::new("k8s://pod/web", "aws://rds/db", Relation::Correlated).with_evidence(ev(
                EvidenceKind::Observed,
                "cgroup + flow log",
                81,
                t0 + Duration::milliseconds(900),
            )),
        );
        (g, t0)
    }

    #[test]
    fn resolves_to_unwraps_target_but_keeps_request_fqdn() {
        let (g, _) = fixture();
        let req = TraceRequest::new("web", "api.example", 443, "HTTPS");
        let r = TraceEngine::trace(&g, &req).unwrap();
        assert_eq!(r.status, TraceStatus::Completed);
        assert_eq!(r.request.to, "api.example");
        let names: Vec<&str> = r.hops.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, vec!["web", "app-svc"]);
        assert_eq!(r.hops[0].edge_in, None);
        assert_eq!(r.hops[0].confidence, Confidence(100));
        assert_eq!(r.hops[1].edge_in, Some(EdgeId(0)));
        assert_eq!(r.hops[1].edge_status, NodeStatus::Observed);
        assert_eq!(r.hops[1].edge_source.as_deref(), Some("eBPF"));
        assert_eq!(r.hops[0].evidence_kind, None);
        assert_eq!(r.hops[0].sources, 0);
        assert_eq!(r.hops[1].evidence_kind, Some(EvidenceKind::Observed));
        assert_eq!(r.hops[1].sources, 1);
        assert_eq!(r.hops[1].confidence, Confidence(92));
        assert_eq!(r.hops[1].metrics.rtt_ms, Some(1.5));
        assert_eq!(r.hops[1].address, "10.0.2.34:443");
        // a entidade DNS desembrulhada não vira branch; db sim
        let branch_names: Vec<&str> = r.branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(branch_names, vec!["db"]);
        assert_eq!(r.branches[0].anchor, 1);
        assert_eq!(r.branches[0].direction, crate::result::Direction::Outgoing);
        assert_eq!(r.branches[0].edge_status, NodeStatus::Inferred);
        assert_eq!(r.correlations.len(), 1);
        assert_eq!(r.correlations[0].score, Confidence(81));
        assert_eq!(r.summary.hops, 2);
        assert_eq!(r.summary.observed, 2);
        assert_eq!(r.summary.inferred, 0);
        assert_eq!(r.summary.correlations, 1);
        assert_eq!(r.summary.confidence, Confidence(92));
        assert_eq!(
            r.summary.duration_ms, 1500,
            "resolution edge is inside the window"
        );
        assert!(r.findings.is_empty());
        assert!(r.hop_for("k8s://svc/app").is_some());

        // sem desembrulhar: o alvo direto é o próprio serviço e a DNS vira branch (Incoming)
        let direct =
            TraceEngine::trace(&g, &TraceRequest::new("web", "app-svc", 443, "HTTPS")).unwrap();
        let branch_names: Vec<&str> = direct.branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(branch_names, vec!["db", "api.example"]);
        assert_eq!(
            direct.branches[1].direction,
            crate::result::Direction::Incoming
        );
        assert!(direct.id > r.id);
    }

    #[test]
    fn errors_for_missing_source_target_and_no_path() {
        let (g, _) = fixture();
        let err =
            TraceEngine::trace(&g, &TraceRequest::new("ghost", "app-svc", 443, "TCP")).unwrap_err();
        assert_eq!(err, TraceError::SourceNotFound("ghost".into()));
        assert_eq!(err.to_string(), "source not found: ghost");

        let err =
            TraceEngine::trace(&g, &TraceRequest::new("web", "ghost", 443, "TCP")).unwrap_err();
        assert_eq!(err, TraceError::TargetNotFound("ghost".into()));

        let err =
            TraceEngine::trace(&g, &TraceRequest::new("web", "lonely", 443, "TCP")).unwrap_err();
        assert_eq!(
            err,
            TraceError::NoPath {
                from: "web".into(),
                to: "lonely".into()
            }
        );
        assert_eq!(err.to_string(), "no path from web to lonely");

        // só arestas de saída contam: db não alcança web
        let err = TraceEngine::trace(&g, &TraceRequest::new("db", "web", 443, "TCP")).unwrap_err();
        assert!(matches!(err, TraceError::NoPath { .. }));
    }

    #[test]
    fn cname_chain_aliases_never_become_branches() {
        // web → svc; dns://a ResolvesTo dns://b ResolvesTo svc; --to dns://a.
        let mut g = Graph::new();
        for (id, kind) in [
            ("web", EntityKind::Pod),
            ("svc", EntityKind::Service),
            ("dns://a", EntityKind::Endpoint),
            ("dns://b", EntityKind::Endpoint),
        ] {
            g.upsert_entity(Entity::new(id, id, kind, Provider::Demo));
        }
        g.add_edge(Edge::new("web", "svc", Relation::Connects));
        g.add_edge(Edge::new("dns://a", "dns://b", Relation::ResolvesTo));
        g.add_edge(Edge::new("dns://b", "svc", Relation::ResolvesTo));

        let r = TraceEngine::trace(&g, &TraceRequest::new("web", "dns://a", 443, "TCP")).unwrap();
        let names: Vec<&str> = r.hops.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, vec!["web", "svc"]);
        assert!(
            r.branches.is_empty(),
            "resolution aliases are not topology: {:?}",
            r.branches
        );

        // Apontando direto para `svc`, nada foi desembrulhado: `dns://b` é um
        // vizinho legítimo (Incoming) e `dns://a` fica fora (não é vizinho).
        let direct = TraceEngine::trace(&g, &TraceRequest::new("web", "svc", 443, "TCP")).unwrap();
        let branch_names: Vec<&str> = direct.branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(branch_names, vec!["dns://b"]);
    }

    #[test]
    fn peer_edges_are_traced_in_both_directions() {
        let mut g = Graph::new();
        for id in ["a", "b"] {
            g.upsert_entity(Entity::new(id, id, EntityKind::Gateway, Provider::Demo));
        }
        let t0 = Utc::now();
        g.add_edge(
            Edge::new("a", "b", Relation::Peer)
                .with_evidence(ev(EvidenceKind::Observed, "BGP session", 90, t0))
                .with_metrics(Metrics {
                    rtt_ms: Some(3.0),
                    retransmissions: 0,
                    drops: 0,
                    rst: 0,
                }),
        );
        let fwd = TraceEngine::trace(&g, &TraceRequest::new("a", "b", 179, "TCP")).unwrap();
        let rev = TraceEngine::trace(&g, &TraceRequest::new("b", "a", 179, "TCP")).unwrap();
        let names =
            |r: &TraceResult| -> Vec<String> { r.hops.iter().map(|h| h.name.clone()).collect() };
        assert_eq!(names(&fwd), vec!["a", "b"]);
        assert_eq!(names(&rev), vec!["b", "a"]);
        // A mesma aresta (mesmo EdgeId) serve os dois sentidos, com a mesma evidência.
        assert_eq!(fwd.hops[1].edge_in, Some(EdgeId(0)));
        assert_eq!(rev.hops[1].edge_in, Some(EdgeId(0)));
        assert_eq!(rev.hops[1].edge_source.as_deref(), Some("BGP session"));
        assert_eq!(rev.hops[1].evidence_kind, Some(EvidenceKind::Observed));
        assert_eq!(rev.hops[1].metrics.rtt_ms, Some(3.0));
        assert_eq!(rev.summary.confidence, fwd.summary.confidence);
        assert!(rev.branches.is_empty());
    }

    #[test]
    fn single_hop_trace_when_source_is_target() {
        let (g, _) = fixture();
        let r = TraceEngine::trace(&g, &TraceRequest::new("web", "web", 443, "TCP")).unwrap();
        assert_eq!(r.hops.len(), 1);
        assert_eq!(r.summary.confidence, Confidence(100));
        assert!(r.render_ascii().starts_with("PEABIRU TRACE #"));
    }
}
