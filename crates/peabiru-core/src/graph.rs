//! Grafo temporal (VISION §3, §14): relações, arestas com evidência e métricas,
//! e o `Graph` global onde discovery, sensors e correlação escrevem.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use indexmap::map::Entry;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::evidence::{merge_evidence, Confidence, Evidence};
use crate::model::{Entity, EntityId, Validity};
use crate::status::NodeStatus;

/// Tipo de relação entre duas entidades.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Relation {
    Connects,
    RoutesTo,
    ForwardsTo,
    Contains,
    ResolvesTo,
    Correlated,
    Peer,
}

impl Relation {
    /// `"connects"`, `"routes_to"`, `"forwards_to"`, `"contains"`, `"resolves_to"`,
    /// `"correlated"`, `"peer"`.
    pub fn label(&self) -> &'static str {
        match self {
            Relation::Connects => "connects",
            Relation::RoutesTo => "routes_to",
            Relation::ForwardsTo => "forwards_to",
            Relation::Contains => "contains",
            Relation::ResolvesTo => "resolves_to",
            Relation::Correlated => "correlated",
            Relation::Peer => "peer",
        }
    }

    /// `"↔"` para `Correlated | Peer`, `"→"` para as demais.
    pub fn arrow(&self) -> &'static str {
        match self {
            Relation::Correlated | Relation::Peer => "↔",
            _ => "→",
        }
    }
}

/// Métricas agregadas de uma aresta.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub rtt_ms: Option<f64>,
    pub retransmissions: u32,
    pub drops: u32,
    pub rst: u32,
}

impl Metrics {
    /// `rtt_ms`: prefere `Some`, máximo se ambos; contadores: máximo.
    pub(crate) fn merge(&mut self, other: &Metrics) {
        self.rtt_ms = match (self.rtt_ms, other.rtt_ms) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        self.retransmissions = self.retransmissions.max(other.retransmissions);
        self.drops = self.drops.max(other.drops);
        self.rst = self.rst.max(other.rst);
    }
}

/// Relação dirigida `from → to`, com evidência de como foi descoberta.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub from: EntityId,
    pub to: EntityId,
    pub relation: Relation,
    pub evidence: Vec<Evidence>,
    pub metrics: Metrics,
    pub validity: Validity,
}

impl Edge {
    pub fn new(from: &str, to: &str, relation: Relation) -> Self {
        Self {
            from: from.to_string(),
            to: to.to_string(),
            relation,
            evidence: Vec::new(),
            metrics: Metrics::default(),
            validity: Validity::always(),
        }
    }

    pub fn with_evidence(mut self, e: Evidence) -> Self {
        self.evidence.push(e);
        self
    }

    pub fn with_metrics(mut self, m: Metrics) -> Self {
        self.metrics = m;
        self
    }

    /// Mesma regra da entidade sobre a evidência da aresta (sem `Probe`).
    pub fn status(&self) -> NodeStatus {
        NodeStatus::from_evidence(&self.evidence)
    }

    /// `max(effective_confidence)` das evidências (0 se nenhuma). O trace engine
    /// aplica sua própria política de fallback (ver `peabiru-trace::confidence`).
    pub fn confidence(&self) -> Confidence {
        self.best_evidence()
            .map(|e| Confidence::from_ratio(e.effective_confidence()))
            .unwrap_or_default()
    }

    /// Evidência de maior confiança efetiva (a primeira em caso de empate).
    pub fn best_evidence(&self) -> Option<&Evidence> {
        self.evidence.iter().fold(None, |best, e| match best {
            Some(b) if b.effective_confidence() >= e.effective_confidence() => Some(b),
            _ => Some(e),
        })
    }

    /// Merge usado por `Graph::add_edge` quando `(from, to, relation)` já existe.
    pub(crate) fn merge(&mut self, other: Edge) {
        merge_evidence(&mut self.evidence, other.evidence);
        self.metrics.merge(&other.metrics);
        self.validity.merge(&other.validity);
    }
}

/// Índice estável de uma aresta dentro do `Graph`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeId(pub usize);

/// Grafo global. Serializa como `{ "entities": {id → Entity}, "edges": [Edge] }`;
/// os índices de adjacência são reconstruídos ao desserializar.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(from = "GraphParts")]
pub struct Graph {
    entities: IndexMap<EntityId, Entity>,
    edges: Vec<Edge>,
    #[serde(skip)]
    out_adj: HashMap<EntityId, Vec<usize>>,
    #[serde(skip)]
    in_adj: HashMap<EntityId, Vec<usize>>,
}

/// Forma serializada do grafo (sem índices).
#[derive(Deserialize)]
struct GraphParts {
    #[serde(default)]
    entities: IndexMap<EntityId, Entity>,
    #[serde(default)]
    edges: Vec<Edge>,
}

impl From<GraphParts> for Graph {
    fn from(parts: GraphParts) -> Self {
        Graph::from_parts(parts.entities, parts.edges)
    }
}

impl PartialEq for Graph {
    fn eq(&self, other: &Self) -> bool {
        self.entities == other.entities && self.edges == other.edges
    }
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Monta um grafo a partir de entidades e arestas já existentes,
    /// reconstruindo os índices de adjacência.
    pub fn from_parts(entities: IndexMap<EntityId, Entity>, edges: Vec<Edge>) -> Self {
        let mut g = Graph {
            entities,
            edges,
            out_adj: HashMap::new(),
            in_adj: HashMap::new(),
        };
        g.reindex();
        g
    }

    fn reindex(&mut self) {
        self.out_adj.clear();
        self.in_adj.clear();
        for i in 0..self.edges.len() {
            self.index_edge(i);
        }
    }

    fn index_edge(&mut self, i: usize) {
        let (from, to) = (self.edges[i].from.clone(), self.edges[i].to.clone());
        self.out_adj.entry(from).or_default().push(i);
        self.in_adj.entry(to).or_default().push(i);
    }

    /// Insere ou mescla (ver `Entity::merge`): endereços e evidência anexados
    /// sem duplicar, attrs sobrescritos, nome mantido salvo se vazio.
    pub fn upsert_entity(&mut self, e: Entity) -> &mut Entity {
        match self.entities.entry(e.id.clone()) {
            Entry::Occupied(slot) => {
                let existing = slot.into_mut();
                existing.merge(e);
                existing
            }
            Entry::Vacant(slot) => slot.insert(e),
        }
    }

    /// Insere ou mescla por `(from, to, relation)` (evidência e métricas).
    /// Não exige que as entidades já existam no grafo.
    pub fn add_edge(&mut self, e: Edge) -> EdgeId {
        let existing = self
            .edges
            .iter()
            .position(|x| x.from == e.from && x.to == e.to && x.relation == e.relation);
        match existing {
            Some(i) => {
                self.edges[i].merge(e);
                EdgeId(i)
            }
            None => {
                let i = self.edges.len();
                self.edges.push(e);
                self.index_edge(i);
                EdgeId(i)
            }
        }
    }

    pub fn entity(&self, id: &str) -> Option<&Entity> {
        self.entities.get(id)
    }

    /// Acesso mutável; alterar `Entity::id` por aqui deixa o grafo inconsistente.
    pub fn entity_mut(&mut self, id: &str) -> Option<&mut Entity> {
        self.entities.get_mut(id)
    }

    /// Entidades na ordem de inserção.
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    /// Arestas na ordem de inserção.
    pub fn edges(&self) -> impl Iterator<Item = (EdgeId, &Edge)> {
        self.edges.iter().enumerate().map(|(i, e)| (EdgeId(i), e))
    }

    pub fn edge(&self, id: EdgeId) -> Option<&Edge> {
        self.edges.get(id.0)
    }

    fn adjacent(&self, index: &HashMap<EntityId, Vec<usize>>, id: &str) -> Vec<(EdgeId, &Edge)> {
        index
            .get(id)
            .map(|ixs| ixs.iter().map(|&i| (EdgeId(i), &self.edges[i])).collect())
            .unwrap_or_default()
    }

    pub fn out_edges(&self, id: &str) -> Vec<(EdgeId, &Edge)> {
        self.adjacent(&self.out_adj, id)
    }

    pub fn in_edges(&self, id: &str) -> Vec<(EdgeId, &Edge)> {
        self.adjacent(&self.in_adj, id)
    }

    /// Vizinhos nas duas direções (saída primeiro, depois entrada), só os que
    /// existem como entidade no grafo. Um self-loop aparece uma vez.
    pub fn neighbors(&self, id: &str) -> Vec<(EdgeId, &Edge, &Entity)> {
        let mut out = Vec::new();
        for (eid, e) in self.out_edges(id) {
            if let Some(ent) = self.entity(&e.to) {
                out.push((eid, e, ent));
            }
        }
        for (eid, e) in self.in_edges(id) {
            if e.from == e.to {
                continue;
            }
            if let Some(ent) = self.entity(&e.from) {
                out.push((eid, e, ent));
            }
        }
        out
    }

    /// Entidades com um endereço de IP `ip`; com `port = Some(p)`, o endereço
    /// precisa ter porta `p` ou não ter porta.
    pub fn find_by_address(&self, ip: IpAddr, port: Option<u16>) -> Vec<&Entity> {
        self.entities
            .values()
            .filter(|e| {
                e.addresses.iter().any(|a| {
                    a.ip == Some(ip) && port.is_none_or(|p| a.port.is_none_or(|ap| ap == p))
                })
            })
            .collect()
    }

    /// Primeira entidade com `name` exatamente igual.
    pub fn find_by_name(&self, name: &str) -> Option<&Entity> {
        self.entities.values().find(|e| e.name == name)
    }

    /// Resolve um selector, nesta ordem: id exato → `name` → `"ip:port"` →
    /// `"ip"` → hostname (case-insensitive) em `addresses`. Primeiro match.
    pub fn resolve(&self, selector: &str) -> Option<&Entity> {
        let sel = selector.trim();
        if sel.is_empty() {
            return None;
        }
        if let Some(e) = self.entity(sel) {
            return Some(e);
        }
        if let Some(e) = self.find_by_name(sel) {
            return Some(e);
        }
        let by_socket = sel.parse::<SocketAddr>().ok().and_then(|sa| {
            self.find_by_address(sa.ip(), Some(sa.port()))
                .first()
                .copied()
        });
        if by_socket.is_some() {
            return by_socket;
        }
        let by_ip = sel
            .parse::<IpAddr>()
            .ok()
            .and_then(|ip| self.find_by_address(ip, None).first().copied());
        if by_ip.is_some() {
            return by_ip;
        }
        self.entities.values().find(|e| {
            e.addresses.iter().any(|a| {
                a.hostname
                    .as_deref()
                    .is_some_and(|h| h.eq_ignore_ascii_case(sel))
            })
        })
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::EvidenceKind;
    use crate::model::{Address, EntityKind, Provider};
    use chrono::{TimeZone, Utc};

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn ev(kind: EvidenceKind, source: &str, pct: u8) -> Evidence {
        Evidence::new(kind, source, Provider::Demo, Confidence(pct))
    }

    fn fixture() -> Graph {
        let mut g = Graph::new();
        g.upsert_entity(
            Entity::new(
                "k8s://prod/pod/web",
                "web-frontend",
                EntityKind::Pod,
                Provider::Kubernetes,
            )
            .with_address(Address::socket(ip("10.42.1.15"), 51542))
            .with_evidence(ev(EvidenceKind::Observed, "eBPF", 95)),
        );
        g.upsert_entity(
            Entity::new(
                "aws://1/alb/alb2c3",
                "aws-alb",
                EntityKind::LoadBalancer,
                Provider::Aws,
            )
            .with_address(Address::host("alb2c3.elb.amazonaws.com"))
            .with_address(Address::socket(ip("52.95.23.10"), 443)),
        );
        g.upsert_entity(
            Entity::new(
                "k8s://prod/service/app-svc",
                "app-svc",
                EntityKind::Service,
                Provider::Kubernetes,
            )
            .with_address(Address::socket(ip("10.0.2.34"), 443)),
        );
        g.upsert_entity(
            Entity::new(
                "aws://1/natgw/nat-0a1b",
                "nat-gw",
                EntityKind::Nat,
                Provider::Aws,
            )
            .with_address(Address::ip(ip("10.0.1.5"))),
        );
        g.add_edge(
            Edge::new(
                "k8s://prod/pod/web",
                "aws://1/alb/alb2c3",
                Relation::Connects,
            )
            .with_evidence(ev(EvidenceKind::Observed, "eBPF", 92)),
        );
        g.add_edge(
            Edge::new(
                "aws://1/alb/alb2c3",
                "k8s://prod/service/app-svc",
                Relation::ForwardsTo,
            )
            .with_evidence(ev(EvidenceKind::LogConfirmed, "VPC Flow Logs", 90)),
        );
        g.add_edge(
            Edge::new(
                "aws://1/alb/alb2c3",
                "aws://1/natgw/nat-0a1b",
                Relation::RoutesTo,
            )
            .with_evidence(ev(EvidenceKind::Inferred, "route inference", 60)),
        );
        g
    }

    #[test]
    fn upsert_merges_entity() {
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 1, 5, 0, 0, 0).unwrap();
        let mut g = Graph::new();
        let first = Entity::new(
            "aws://1/alb/alb2c3",
            "aws-alb",
            EntityKind::LoadBalancer,
            Provider::Aws,
        )
        .with_address(Address::host("alb2c3.elb.amazonaws.com"))
        .with_attr("region", "us-east-1")
        .with_attr("type", "load_balancer")
        .with_evidence(ev(EvidenceKind::Observed, "eBPF (node/ip-10-0-1-25)", 92).at(t0));
        let mut first = first;
        first.validity.valid_from = Some(t1);
        g.upsert_entity(first);

        let second = Entity::new(
            "aws://1/alb/alb2c3",
            "",
            EntityKind::LoadBalancer,
            Provider::Aws,
        )
        .with_address(Address::host("alb2c3.elb.amazonaws.com"))
        .with_address(Address::socket(ip("52.95.23.10"), 443))
        .with_attr("type", "load_balancer (aws_alb)")
        .with_attr("as", "AS16509")
        .with_evidence(ev(EvidenceKind::Observed, "eBPF (node/ip-10-0-1-25)", 92).at(t1))
        .with_evidence(ev(EvidenceKind::LogConfirmed, "VPC Flow Logs", 90).at(t1));
        let mut second = second;
        second.validity.valid_from = Some(t0);
        let merged = g.upsert_entity(second);

        assert_eq!(merged.name, "aws-alb", "existing non-empty name is kept");
        assert_eq!(merged.addresses.len(), 2, "duplicate address not appended");
        assert_eq!(
            merged.attrs.get("region").map(String::as_str),
            Some("us-east-1")
        );
        assert_eq!(
            merged.attrs.get("type").map(String::as_str),
            Some("load_balancer (aws_alb)")
        );
        assert_eq!(merged.attrs.get("as").map(String::as_str), Some("AS16509"));
        assert_eq!(
            merged.evidence.len(),
            2,
            "same (kind, source) not duplicated"
        );
        assert_eq!(
            merged.evidence[0].timestamp, t1,
            "refreshed to the latest sighting"
        );
        assert_eq!(
            merged.validity.valid_from,
            Some(t0),
            "earliest valid_from wins"
        );
        assert_eq!(merged.status(), NodeStatus::Correlated);
        assert_eq!(g.entity_count(), 1);

        // name gets filled when the stored one is empty; Unknown kind/provider promoted
        let mut g = Graph::new();
        g.upsert_entity(Entity::new(
            "ip://10.0.3.8",
            "",
            EntityKind::Unknown,
            Provider::Unknown,
        ));
        let e = g.upsert_entity(Entity::new(
            "ip://10.0.3.8",
            "redis",
            EntityKind::Cache,
            Provider::Aws,
        ));
        assert_eq!(e.name, "redis");
        assert_eq!(e.kind, EntityKind::Cache);
        assert_eq!(e.provider, Provider::Aws);
    }

    #[test]
    fn add_edge_dedups_and_merges() {
        let mut g = Graph::new();
        let a = g.add_edge(
            Edge::new("a", "b", Relation::Connects)
                .with_evidence(ev(EvidenceKind::Observed, "eBPF", 90))
                .with_metrics(Metrics {
                    rtt_ms: None,
                    retransmissions: 2,
                    drops: 0,
                    rst: 1,
                }),
        );
        let b = g.add_edge(
            Edge::new("a", "b", Relation::Connects)
                .with_evidence(ev(EvidenceKind::Observed, "eBPF", 90))
                .with_evidence(ev(EvidenceKind::LogConfirmed, "VPC Flow Logs", 85))
                .with_metrics(Metrics {
                    rtt_ms: Some(1.217),
                    retransmissions: 1,
                    drops: 3,
                    rst: 0,
                }),
        );
        assert_eq!(a, b);
        assert_eq!(g.edge_count(), 1);
        let e = g.edge(a).unwrap();
        assert_eq!(e.evidence.len(), 2);
        assert_eq!(e.metrics.rtt_ms, Some(1.217));
        assert_eq!(e.metrics.retransmissions, 2);
        assert_eq!(e.metrics.drops, 3);
        assert_eq!(e.metrics.rst, 1);
        assert_eq!(e.status(), NodeStatus::Correlated);

        // rtt: max when both present
        g.add_edge(
            Edge::new("a", "b", Relation::Connects).with_metrics(Metrics {
                rtt_ms: Some(0.5),
                ..Metrics::default()
            }),
        );
        assert_eq!(g.edge(a).unwrap().metrics.rtt_ms, Some(1.217));

        // different relation or direction → new edge
        let c = g.add_edge(Edge::new("a", "b", Relation::Correlated));
        let d = g.add_edge(Edge::new("b", "a", Relation::Connects));
        assert_ne!(a, c);
        assert_ne!(c, d);
        assert_eq!(g.edge_count(), 3);
        assert_eq!(g.out_edges("a").len(), 2);
        assert_eq!(g.in_edges("a").len(), 1);
        assert_eq!(g.out_edges("nobody").len(), 0);
    }

    #[test]
    fn edge_status_and_confidence() {
        let e = Edge::new("a", "b", Relation::Connects);
        assert_eq!(e.status(), NodeStatus::Inferred);
        assert_eq!(e.confidence(), Confidence(0));
        assert!(e.best_evidence().is_none());

        let e = e
            .with_evidence(ev(EvidenceKind::Inferred, "route inference", 100))
            .with_evidence(ev(EvidenceKind::LogConfirmed, "VPC Flow Logs", 90))
            .with_evidence(ev(EvidenceKind::Observed, "eBPF", 80));
        assert_eq!(e.status(), NodeStatus::Correlated);
        // Observed 80 → 0.80, LogConfirmed 90 → 0.855, Inferred 100 → 0.70
        assert_eq!(
            e.best_evidence().map(|b| b.source.as_str()),
            Some("VPC Flow Logs")
        );
        assert_eq!(e.confidence(), Confidence(86));

        let single = Edge::new("a", "b", Relation::RoutesTo).with_evidence(ev(
            EvidenceKind::Observed,
            "eBPF",
            92,
        ));
        assert_eq!(single.status(), NodeStatus::Observed);
        assert_eq!(single.confidence(), Confidence(92));
        assert_eq!(Relation::Correlated.arrow(), "↔");
        assert_eq!(Relation::RoutesTo.arrow(), "→");
        assert_eq!(Relation::ForwardsTo.label(), "forwards_to");
    }

    #[test]
    fn neighbors_both_directions() {
        let g = fixture();
        let n = g.neighbors("aws://1/alb/alb2c3");
        let names: Vec<&str> = n.iter().map(|(_, _, e)| e.name.as_str()).collect();
        assert_eq!(names, vec!["app-svc", "nat-gw", "web-frontend"]);
        assert_eq!(n[2].1.relation, Relation::Connects);

        let mut g = g;
        g.add_edge(Edge::new(
            "aws://1/alb/alb2c3",
            "aws://1/alb/alb2c3",
            Relation::Peer,
        ));
        g.add_edge(Edge::new(
            "aws://1/alb/alb2c3",
            "ghost://missing",
            Relation::Connects,
        ));
        let n = g.neighbors("aws://1/alb/alb2c3");
        assert_eq!(n.len(), 4, "self-loop once, dangling edge skipped");
    }

    #[test]
    fn resolve_selectors() {
        let g = fixture();
        assert_eq!(
            g.resolve("k8s://prod/pod/web").map(|e| e.name.as_str()),
            Some("web-frontend")
        );
        assert_eq!(
            g.resolve("web-frontend").map(|e| e.id.as_str()),
            Some("k8s://prod/pod/web")
        );
        assert_eq!(
            g.resolve("10.0.2.34:443").map(|e| e.name.as_str()),
            Some("app-svc")
        );
        assert_eq!(
            g.resolve("10.0.2.34").map(|e| e.name.as_str()),
            Some("app-svc")
        );
        assert_eq!(g.resolve("10.0.2.34:8080"), None, "port mismatch");
        assert_eq!(
            g.resolve("10.0.1.5:22").map(|e| e.name.as_str()),
            Some("nat-gw"),
            "portless address matches any port"
        );
        assert_eq!(
            g.resolve("ALB2C3.elb.amazonaws.com")
                .map(|e| e.name.as_str()),
            Some("aws-alb")
        );
        assert_eq!(g.resolve("  aws-alb "), g.find_by_name("aws-alb"));
        assert_eq!(g.resolve(""), None);
        assert_eq!(g.resolve("nope"), None);
        assert_eq!(g.find_by_address(ip("52.95.23.10"), Some(443)).len(), 1);
        assert_eq!(g.find_by_address(ip("52.95.23.10"), Some(80)).len(), 0);
        assert_eq!(g.find_by_address(ip("1.1.1.1"), None).len(), 0);
    }

    #[test]
    fn serde_round_trip_rebuilds_adjacency() {
        let g = fixture();
        // Round-trip through text: `serde_json::Value` objects are BTreeMap-backed
        // and would re-sort the entity keys on their own.
        let text = serde_json::to_string(&g).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut keys: Vec<&String> = json.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec!["edges", "entities"],
            "no adjacency in the wire format"
        );
        assert!(json["entities"]["aws://1/alb/alb2c3"].is_object());
        assert_eq!(json["edges"].as_array().unwrap().len(), 3);

        let back: Graph = serde_json::from_str(&text).unwrap();
        assert_eq!(back, g);
        assert_eq!(back.entity_count(), 4);
        assert_eq!(back.edge_count(), 3);
        assert_eq!(back.out_edges("aws://1/alb/alb2c3").len(), 2);
        assert_eq!(back.in_edges("aws://1/alb/alb2c3").len(), 1);
        assert_eq!(back.neighbors("k8s://prod/service/app-svc").len(), 1);
        let ids: Vec<&str> = back.entities().map(|e| e.id.as_str()).collect();
        assert_eq!(ids[0], "k8s://prod/pod/web", "insertion order preserved");

        let empty: Graph = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.entity_count(), 0);
        let mut g2 = empty;
        g2.add_edge(Edge::new("x", "y", Relation::Connects));
        assert_eq!(g2.out_edges("x").len(), 1);
    }
}
