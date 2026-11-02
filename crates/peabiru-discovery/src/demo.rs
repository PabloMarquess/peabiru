//! Provider e sensor `demo`: a fixture que reproduz os mockups
//! (ARCHITECTURE §4.1). Sem rede e sem credenciais: só o grafo e a timeline.

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use chrono::{TimeDelta, Utc};
use peabiru_core::{
    Address, Confidence, Edge, Entity, EntityKind, Event, EventSource, Evidence, EvidenceKind,
    Graph, Metrics, Provider as CoreProvider, Relation, Timestamp,
};
use peabiru_trace::TraceRequest;

use crate::{DiscoveryReport, Provider, Sensor};

/// Ids das entidades da fixture.
pub const WEB_FRONTEND: &str = "k8s://prod/pod/web-frontend-7f9c8c";
pub const KUBE_PROXY: &str = "k8s://prod/node/kube-proxy";
pub const ISTIO_INGRESS: &str = "k8s://istio-system/pod/istio-ingressgateway";
pub const NAT_GW: &str = "aws://123456789012/natgw/nat-0a1b";
pub const AWS_ALB: &str = "aws://123456789012/alb/alb2c3";
pub const APP_SVC: &str = "k8s://prod/service/app-svc";
pub const REDIS: &str = "ip://10.0.3.8";
pub const ORDERS_DB: &str = "aws://123456789012/rds/orders-db";
pub const PAYMENTS_SVC: &str = "k8s://prod/service/payments-svc";
pub const API_DNS: &str = "dns://api.prod.svc.cluster.local";

/// Duração da timeline do demo (offset do último evento), em ms.
pub const DEMO_DURATION_MS: u64 = 2843;

/// Fontes de evidência usadas na fixture.
const EBPF_NODE_42_1_1: &str = "eBPF (node/ip-10-42-1-1)";
const EBPF_NODE_42_0_1: &str = "eBPF (node/ip-10-42-0-1)";
const EBPF_NODE_0_1_25: &str = "eBPF (node/ip-10-0-1-25)";
const EBPF_NODE_0_2_34: &str = "eBPF (node/ip-10-0-2-34)";
const VPC_FLOW_LOGS: &str = "VPC Flow Logs";
const AWS_ROUTE_TABLE: &str = "AWS Route Table";
const KUBERNETES_API: &str = "Kubernetes API";
const KUBERNETES_DNS: &str = "Kubernetes DNS";
const ROUTE_INFERENCE: &str = "route inference";

/// Provider `demo`: constrói a fixture dos mockups no grafo.
pub struct DemoProvider;

impl Provider for DemoProvider {
    fn name(&self) -> &'static str {
        "demo"
    }

    fn provider(&self) -> CoreProvider {
        CoreProvider::Demo
    }

    /// Timestamps das evidências = agora + offset da timeline (§4.1), de modo
    /// que a janela de observação do trace demo dá `DEMO_DURATION_MS`.
    fn discover(&self, graph: &mut Graph) -> anyhow::Result<DiscoveryReport> {
        Ok(populate(graph, Utc::now()))
    }
}

/// Escreve a fixture (evidências em `base + offset`) em `graph`.
fn populate(graph: &mut Graph, base: Timestamp) -> DiscoveryReport {
    let (entities, edges) = fixture(base);
    let report = DiscoveryReport {
        entities: entities.len(),
        edges: edges.len(),
        warnings: Vec::new(),
    };
    for e in entities {
        graph.upsert_entity(e);
    }
    for e in edges {
        graph.add_edge(e);
    }
    report
}

/// Grafo novo já populado pelo `DemoProvider`.
pub fn graph() -> Graph {
    graph_from(Utc::now())
}

/// Como `graph()`, mas com as evidências carimbadas em `base + offset` em
/// vez de `agora + offset`, para renders determinísticos (snapshots).
pub fn graph_from(base: Timestamp) -> Graph {
    let mut g = Graph::new();
    populate(&mut g, base);
    g
}

/// `from "web-frontend-7f9c8c"`, `to "api.prod.svc.cluster.local"`, porta 443, HTTPS.
pub fn default_request() -> TraceRequest {
    TraceRequest::new(
        "web-frontend-7f9c8c",
        "api.prod.svc.cluster.local",
        443,
        "HTTPS",
    )
}

/// Ambiente exibido no header/rodapé da UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Environment {
    pub environment: String,
    pub cluster: String,
    pub user: String,
    pub endpoint: String,
    pub connected: bool,
}

/// `"prod"`, `"prod-e1"`, `"admin"`, `"peabiru.local:443"`, conectado.
pub fn environment() -> Environment {
    Environment {
        environment: "prod".into(),
        cluster: "prod-e1".into(),
        user: "admin".into(),
        endpoint: "peabiru.local:443".into(),
        connected: true,
    }
}

/// Timeline completa (§4.1) com `ts = agora + offset`.
pub fn events() -> Vec<Event> {
    events_from(Utc::now())
}

/// Timeline completa com `ts = base + offset`, na ordem da tabela.
pub fn events_from(base: Timestamp) -> Vec<Event> {
    let row = |ms: i64, source, action: &str, from: &str, to: &str, bi: bool, detail: &str| Event {
        ts: at(base, ms),
        source,
        action: action.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        bidirectional: bi,
        detail: detail.to_string(),
    };
    vec![
        row(
            0,
            EventSource::Ebpf,
            "connect",
            "web-frontend-7f9c8c:51542",
            "kube-proxy:15001",
            false,
            "1.217ms",
        ),
        row(
            120,
            EventSource::Ebpf,
            "connect",
            "kube-proxy:15001",
            "istio-ingressgateway:15001",
            false,
            "0.892ms",
        ),
        row(
            400,
            EventSource::Correlation,
            "match",
            "istio-ingressgateway",
            "aws-alb",
            true,
            "92%",
        ),
        row(
            1100,
            EventSource::FlowLog,
            "accept",
            "52.95.23.10:443",
            "10.0.2.34:443",
            false,
            "1.103ms",
        ),
        row(
            2200,
            EventSource::Inference,
            "route",
            "10.0.2.34:443",
            "10.0.4.12:5432",
            false,
            "(inferred)",
        ),
        row(
            2500,
            EventSource::Inference,
            "route",
            "10.0.2.34:443",
            "10.0.3.8:6379",
            false,
            "(inferred)",
        ),
        row(
            2843,
            EventSource::Discovery,
            "resolve",
            "api.prod.svc.cluster.local",
            "app-svc",
            false,
            "Kubernetes DNS",
        ),
    ]
}

/// Sensor `demo`: replay de `events()`, um a cada `interval` de tempo de
/// parede desde a criação (ou o último `reset`). O primeiro evento fica
/// disponível imediatamente; o k-ésimo após `k * interval`. Os `ts` dos
/// eventos seguem a timeline (`início + offset`), não o instante do `poll`.
pub struct DemoSensor {
    interval: Duration,
    started: Instant,
    next: usize,
    events: Vec<Event>,
    /// Base fixa da timeline (`None`: `Utc::now()` a cada `new`/`reset`).
    base: Option<Timestamp>,
}

impl DemoSensor {
    pub fn new(interval: Duration) -> Self {
        Self::build(interval, None)
    }

    /// Como `new`, mas os `ts` dos eventos partem de `base` em vez de agora
    /// (renders determinísticos); `reset` mantém a base.
    pub fn with_base(interval: Duration, base: Timestamp) -> Self {
        Self::build(interval, Some(base))
    }

    fn build(interval: Duration, base: Option<Timestamp>) -> Self {
        Self {
            interval,
            started: Instant::now(),
            next: 0,
            events: events_from(base.unwrap_or_else(Utc::now)),
            base,
        }
    }

    /// Recomeça o replay do zero (novo início, nova timeline).
    pub fn reset(&mut self) {
        *self = Self::build(self.interval, self.base);
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Eventos ainda não emitidos.
    pub fn remaining(&self) -> usize {
        self.events.len().saturating_sub(self.next)
    }

    /// `true` quando todos os eventos já foram emitidos.
    pub fn is_finished(&self) -> bool {
        self.remaining() == 0
    }

    /// Quantos eventos já deveriam ter sido emitidos até agora.
    fn due(&self) -> usize {
        if self.interval.is_zero() {
            return self.events.len();
        }
        let ticks = self.started.elapsed().as_nanos() / self.interval.as_nanos();
        let due = usize::try_from(ticks.saturating_add(1)).unwrap_or(usize::MAX);
        due.min(self.events.len())
    }
}

impl Sensor for DemoSensor {
    fn name(&self) -> &'static str {
        "demo"
    }

    fn poll(&mut self) -> Vec<Event> {
        let due = self.due();
        if due <= self.next {
            return Vec::new();
        }
        let batch = self.events[self.next..due].to_vec();
        self.next = due;
        batch
    }
}

fn at(base: Timestamp, offset_ms: i64) -> Timestamp {
    base + TimeDelta::milliseconds(offset_ms)
}

fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

fn ev(
    kind: EvidenceKind,
    source: &str,
    provider: CoreProvider,
    pct: u8,
    base: Timestamp,
    offset_ms: i64,
) -> Evidence {
    Evidence::new(kind, source, provider, Confidence::new(pct)).at(at(base, offset_ms))
}

fn rtt(ms: f64) -> Metrics {
    Metrics {
        rtt_ms: Some(ms),
        ..Metrics::default()
    }
}

/// Chave de metadata onde as arestas `Correlated` guardam o texto do `basis`
/// (lido por `peabiru_trace::correlation::correlation_basis`).
pub const BASIS_META: &str = "basis";

/// Aresta `Correlated` com as **duas** evidências que a correlação junta
/// (fontes observacionais distintas, logo `status()` = `Correlated`, nunca
/// `Observed`) e o texto do `basis` em `metadata["basis"]` de ambas. A
/// primeira evidência é a de maior confiança efetiva, ou seja, o score.
fn correlated(from: &str, to: &str, basis: &str, primary: Evidence, secondary: Evidence) -> Edge {
    Edge::new(from, to, Relation::Correlated)
        .with_evidence(primary.with_meta(BASIS_META, basis))
        .with_evidence(secondary.with_meta(BASIS_META, basis))
}

/// Entidades e arestas da fixture (§4.1), com evidências carimbadas em
/// `base + offset` seguindo a timeline dos eventos.
fn fixture(base: Timestamp) -> (Vec<Entity>, Vec<Edge>) {
    use CoreProvider::{Aws, Kubernetes, Unknown};
    use EvidenceKind::{Discovered, Inferred, LogConfirmed, Observed};

    let entities = vec![
        Entity::new(
            WEB_FRONTEND,
            "web-frontend-7f9c8c",
            EntityKind::Pod,
            Kubernetes,
        )
        .with_address(Address::socket(v4(10, 42, 1, 15), 51542))
        .with_attr("namespace", "prod")
        .with_attr("node", "ip-10-42-1-1")
        .with_evidence(ev(Observed, EBPF_NODE_42_1_1, Kubernetes, 95, base, 0)),
        Entity::new(KUBE_PROXY, "kube-proxy", EntityKind::Proxy, Kubernetes)
            .with_address(Address::ip(v4(10, 42, 1, 1)))
            .with_attr("node", "ip-10-42-1-1")
            .with_evidence(ev(Observed, EBPF_NODE_42_1_1, Kubernetes, 95, base, 0)),
        Entity::new(
            ISTIO_INGRESS,
            "istio-ingressgateway",
            EntityKind::Gateway,
            Kubernetes,
        )
        .with_address(Address::socket(v4(10, 42, 0, 7), 15001))
        .with_attr("namespace", "istio-system")
        .with_evidence(ev(Observed, EBPF_NODE_42_0_1, Kubernetes, 95, base, 120)),
        Entity::new(NAT_GW, "nat-gw", EntityKind::Nat, Aws)
            .with_address(Address::ip(v4(10, 0, 1, 5)))
            .with_attr("region", "us-east-1")
            .with_attr("type", "nat (aws_nat_gateway)")
            .with_evidence(ev(Discovered, AWS_ROUTE_TABLE, Aws, 80, base, 400)),
        Entity::new(AWS_ALB, "aws-alb", EntityKind::LoadBalancer, Aws)
            .with_address(Address::host("alb2c3.elb.amazonaws.com"))
            .with_address(Address::socket(v4(52, 95, 23, 10), 443))
            .with_attr("type", "load_balancer (aws_alb)")
            .with_attr("as", "AS16509 (Amazon.com, Inc.)")
            .with_attr("region", "us-east-1")
            .with_attr("reachable", "true")
            .with_evidence(ev(Observed, EBPF_NODE_0_1_25, Aws, 92, base, 400))
            .with_evidence(ev(LogConfirmed, VPC_FLOW_LOGS, Aws, 90, base, 1100)),
        Entity::new(APP_SVC, "app-svc", EntityKind::Service, Kubernetes)
            .with_address(Address::socket(v4(10, 0, 2, 34), 443))
            .with_attr("namespace", "prod")
            .with_attr("reachable", "true")
            .with_evidence(ev(Observed, EBPF_NODE_0_2_34, Kubernetes, 95, base, 1100))
            .with_evidence(ev(Discovered, KUBERNETES_API, Kubernetes, 90, base, 0)),
        Entity::new(REDIS, "redis", EntityKind::Cache, Unknown)
            .with_address(Address::socket(v4(10, 0, 3, 8), 6379))
            .with_evidence(ev(Inferred, ROUTE_INFERENCE, Unknown, 60, base, 2500)),
        Entity::new(ORDERS_DB, "orders-db", EntityKind::Database, Aws)
            .with_address(Address::socket(v4(10, 0, 4, 12), 5432))
            .with_attr("region", "us-east-1")
            .with_attr("type", "database (aws_rds)")
            .with_evidence(ev(LogConfirmed, VPC_FLOW_LOGS, Aws, 90, base, 2200))
            .with_evidence(ev(Observed, EBPF_NODE_0_2_34, Kubernetes, 88, base, 2200)),
        Entity::new(
            PAYMENTS_SVC,
            "payments-svc",
            EntityKind::Service,
            Kubernetes,
        )
        .with_address(Address::socket(v4(10, 0, 2, 55), 8080))
        .with_attr("namespace", "prod")
        .with_evidence(ev(Discovered, KUBERNETES_API, Kubernetes, 90, base, 0)),
        Entity::new(
            API_DNS,
            "api.prod.svc.cluster.local",
            EntityKind::Endpoint,
            Kubernetes,
        )
        .with_address(Address::host("api.prod.svc.cluster.local"))
        .with_attr("namespace", "prod")
        .with_evidence(ev(Discovered, KUBERNETES_DNS, Kubernetes, 90, base, 2843)),
    ];

    let edges = vec![
        // caminho principal
        Edge::new(WEB_FRONTEND, KUBE_PROXY, Relation::Connects)
            .with_evidence(ev(Observed, EBPF_NODE_42_1_1, Kubernetes, 95, base, 0))
            .with_metrics(rtt(1.217)),
        Edge::new(KUBE_PROXY, ISTIO_INGRESS, Relation::Connects)
            .with_evidence(ev(Observed, EBPF_NODE_42_1_1, Kubernetes, 95, base, 120))
            .with_metrics(rtt(0.892)),
        Edge::new(ISTIO_INGRESS, AWS_ALB, Relation::RoutesTo)
            .with_evidence(ev(Observed, EBPF_NODE_0_1_25, Aws, 92, base, 400))
            .with_evidence(ev(LogConfirmed, VPC_FLOW_LOGS, Aws, 90, base, 1100))
            .with_metrics(Metrics {
                retransmissions: 2,
                ..rtt(1.103)
            }),
        Edge::new(AWS_ALB, APP_SVC, Relation::ForwardsTo)
            .with_evidence(ev(LogConfirmed, VPC_FLOW_LOGS, Aws, 90, base, 1100))
            .with_metrics(rtt(1.103)),
        Edge::new(API_DNS, APP_SVC, Relation::ResolvesTo).with_evidence(ev(
            Discovered,
            KUBERNETES_DNS,
            Kubernetes,
            90,
            base,
            2843,
        )),
        // branches: a ordem de inserção das arestas de saída do app-svc é a
        // ordem das branches no trace e no desenho, ou seja, orders-db
        // (correlada, sob o app-svc), redis (esquerda) e payments-svc (direita).
        Edge::new(ISTIO_INGRESS, NAT_GW, Relation::RoutesTo).with_evidence(ev(
            Discovered,
            AWS_ROUTE_TABLE,
            Aws,
            80,
            base,
            400,
        )),
        Edge::new(APP_SVC, ORDERS_DB, Relation::Connects).with_evidence(ev(
            Inferred,
            ROUTE_INFERENCE,
            Aws,
            65,
            base,
            2200,
        )),
        Edge::new(APP_SVC, REDIS, Relation::Connects).with_evidence(ev(
            Inferred,
            ROUTE_INFERENCE,
            Unknown,
            60,
            base,
            2500,
        )),
        Edge::new(APP_SVC, PAYMENTS_SVC, Relation::Connects).with_evidence(ev(
            Inferred,
            ROUTE_INFERENCE,
            Kubernetes,
            55,
            base,
            2200,
        )),
        // correlações: duas fontes reais por aresta (status Correlated); a
        // Observed de maior confiança fixa o score (peso 1.0 ⇒ 92/81/73).
        correlated(
            AWS_ALB,
            APP_SVC,
            "VPC Flow Logs + eBPF 5-tuple",
            ev(Observed, EBPF_NODE_0_2_34, Kubernetes, 92, base, 1100),
            ev(LogConfirmed, VPC_FLOW_LOGS, Aws, 90, base, 1100),
        ),
        correlated(
            WEB_FRONTEND,
            ORDERS_DB,
            "cgroup + flow log",
            ev(Observed, EBPF_NODE_42_1_1, Kubernetes, 81, base, 2200),
            ev(LogConfirmed, VPC_FLOW_LOGS, Aws, 80, base, 2200),
        ),
        correlated(
            ISTIO_INGRESS,
            REDIS,
            "timing correlation",
            ev(Observed, EBPF_NODE_42_0_1, Kubernetes, 73, base, 2500),
            ev(Observed, EBPF_NODE_0_2_34, Kubernetes, 70, base, 2500),
        ),
    ];

    (entities, edges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use peabiru_core::NodeStatus;

    #[test]
    fn fixture_matches_contract_table() {
        let g = graph();
        assert_eq!(g.entity_count(), 10);
        assert_eq!(g.edge_count(), 12);
        let expect = [
            (
                WEB_FRONTEND,
                "web-frontend-7f9c8c",
                EntityKind::Pod,
                CoreProvider::Kubernetes,
                "10.42.1.15:51542",
                NodeStatus::Observed,
            ),
            (
                KUBE_PROXY,
                "kube-proxy",
                EntityKind::Proxy,
                CoreProvider::Kubernetes,
                "10.42.1.1",
                NodeStatus::Observed,
            ),
            (
                ISTIO_INGRESS,
                "istio-ingressgateway",
                EntityKind::Gateway,
                CoreProvider::Kubernetes,
                "10.42.0.7:15001",
                NodeStatus::Observed,
            ),
            (
                NAT_GW,
                "nat-gw",
                EntityKind::Nat,
                CoreProvider::Aws,
                "10.0.1.5",
                NodeStatus::Inferred,
            ),
            (
                AWS_ALB,
                "aws-alb",
                EntityKind::LoadBalancer,
                CoreProvider::Aws,
                "alb2c3.elb.amazonaws.com",
                NodeStatus::Correlated,
            ),
            (
                APP_SVC,
                "app-svc",
                EntityKind::Service,
                CoreProvider::Kubernetes,
                "10.0.2.34:443",
                NodeStatus::Observed,
            ),
            (
                REDIS,
                "redis",
                EntityKind::Cache,
                CoreProvider::Unknown,
                "10.0.3.8:6379",
                NodeStatus::Inferred,
            ),
            (
                ORDERS_DB,
                "orders-db",
                EntityKind::Database,
                CoreProvider::Aws,
                "10.0.4.12:5432",
                NodeStatus::Correlated,
            ),
            (
                PAYMENTS_SVC,
                "payments-svc",
                EntityKind::Service,
                CoreProvider::Kubernetes,
                "10.0.2.55:8080",
                NodeStatus::Inferred,
            ),
            (
                API_DNS,
                "api.prod.svc.cluster.local",
                EntityKind::Endpoint,
                CoreProvider::Kubernetes,
                "api.prod.svc.cluster.local",
                NodeStatus::Inferred,
            ),
        ];
        for (id, name, kind, provider, addr, status) in expect {
            let e = g.entity(id).unwrap_or_else(|| panic!("missing {id}"));
            assert_eq!(e.name, name);
            assert_eq!(e.kind, kind, "{name}");
            assert_eq!(e.provider, provider, "{name}");
            assert_eq!(e.primary_address().unwrap().display(), addr, "{name}");
            assert_eq!(e.status(), status, "{name}");
        }
        let alb = g.entity(AWS_ALB).unwrap();
        assert_eq!(alb.addresses[1].display(), "52.95.23.10:443");
        assert_eq!(
            alb.attrs.get("as").map(String::as_str),
            Some("AS16509 (Amazon.com, Inc.)")
        );
        assert_eq!(
            alb.attrs.get("type").map(String::as_str),
            Some("load_balancer (aws_alb)")
        );
        assert_eq!(
            alb.attrs.get("region").map(String::as_str),
            Some("us-east-1")
        );
        assert_eq!(alb.attrs.get("reachable").map(String::as_str), Some("true"));
        assert_eq!(alb.confidence(), Confidence(92));
        assert_eq!(alb.sources().len(), 2);
        assert_eq!(
            g.resolve("api.prod.svc.cluster.local")
                .map(|e| e.id.as_str()),
            Some(API_DNS)
        );
        assert_eq!(
            g.resolve("web-frontend-7f9c8c").map(|e| e.id.as_str()),
            Some(WEB_FRONTEND)
        );

        let relation = |from: &str, to: &str| {
            g.out_edges(from)
                .into_iter()
                .find(|(_, e)| e.to == to && e.relation != Relation::Correlated)
                .map(|(_, e)| (e.relation, e.status(), e.metrics.rtt_ms))
        };
        assert_eq!(
            relation(WEB_FRONTEND, KUBE_PROXY),
            Some((Relation::Connects, NodeStatus::Observed, Some(1.217)))
        );
        assert_eq!(
            relation(KUBE_PROXY, ISTIO_INGRESS),
            Some((Relation::Connects, NodeStatus::Observed, Some(0.892)))
        );
        assert_eq!(
            relation(ISTIO_INGRESS, AWS_ALB),
            Some((Relation::RoutesTo, NodeStatus::Correlated, Some(1.103)))
        );
        assert_eq!(
            relation(AWS_ALB, APP_SVC),
            Some((Relation::ForwardsTo, NodeStatus::Observed, Some(1.103)))
        );
        assert_eq!(
            relation(API_DNS, APP_SVC),
            Some((Relation::ResolvesTo, NodeStatus::Inferred, None))
        );
        assert_eq!(
            relation(ISTIO_INGRESS, NAT_GW),
            Some((Relation::RoutesTo, NodeStatus::Inferred, None))
        );
        assert_eq!(
            relation(APP_SVC, REDIS),
            Some((Relation::Connects, NodeStatus::Inferred, None))
        );
        assert_eq!(
            relation(APP_SVC, ORDERS_DB),
            Some((Relation::Connects, NodeStatus::Inferred, None))
        );
        assert_eq!(
            relation(APP_SVC, PAYMENTS_SVC),
            Some((Relation::Connects, NodeStatus::Inferred, None))
        );

        // A aresta 3 carrega as 2 retransmissões do mockup (VISION §24).
        let edge3 = g
            .out_edges(ISTIO_INGRESS)
            .into_iter()
            .find(|(_, e)| e.to == AWS_ALB && e.relation == Relation::RoutesTo)
            .map(|(_, e)| e.metrics.clone())
            .unwrap();
        assert_eq!(edge3.retransmissions, 2);
        assert_eq!(edge3.rtt_ms, Some(1.103));
        assert_eq!((edge3.drops, edge3.rst), (0, 0));
        let other_retx: u32 = g
            .edges()
            .filter(|(_, e)| !(e.from == ISTIO_INGRESS && e.to == AWS_ALB))
            .map(|(_, e)| e.metrics.retransmissions)
            .sum();
        assert_eq!(other_retx, 0, "só a aresta 3 retransmite");

        // Ordem das branches do app-svc = ordem do mockup (orders-db sob o
        // app-svc, redis à esquerda, payments-svc à direita).
        let app_branches: Vec<&str> = g
            .out_edges(APP_SVC)
            .into_iter()
            .filter(|(_, e)| e.relation != Relation::Correlated)
            .map(|(_, e)| e.to.as_str())
            .collect();
        assert_eq!(app_branches, vec![ORDERS_DB, REDIS, PAYMENTS_SVC]);

        // Correlações: duas fontes observacionais distintas por aresta
        // (status Correlated, nunca Observed), score = confidence(), basis em
        // metadata["basis"] de toda evidência.
        let correlated: Vec<(u8, NodeStatus, usize, &str)> = g
            .edges()
            .filter(|(_, e)| e.relation == Relation::Correlated)
            .map(|(_, e)| {
                let bases: Vec<&str> = e
                    .evidence
                    .iter()
                    .map(|ev| ev.metadata.get(BASIS_META).map_or("", String::as_str))
                    .collect();
                assert_eq!(bases.len(), 2, "{} -> {}", e.from, e.to);
                assert_eq!(bases[0], bases[1], "{} -> {}", e.from, e.to);
                (
                    e.confidence().pct(),
                    e.status(),
                    peabiru_core::observational_sources(&e.evidence).len(),
                    bases[0],
                )
            })
            .collect();
        assert_eq!(
            correlated,
            vec![
                (
                    92,
                    NodeStatus::Correlated,
                    2,
                    "VPC Flow Logs + eBPF 5-tuple"
                ),
                (81, NodeStatus::Correlated, 2, "cgroup + flow log"),
                (73, NodeStatus::Correlated, 2, "timing correlation")
            ]
        );
    }

    #[test]
    fn fixed_base_is_deterministic() {
        use chrono::TimeZone;
        let base = Utc.with_ymd_and_hms(2026, 1, 1, 12, 41, 2).unwrap();
        assert_eq!(graph_from(base), graph_from(base));
        let alb = graph_from(base);
        let newest = alb
            .entity(AWS_ALB)
            .unwrap()
            .evidence
            .iter()
            .map(|e| e.timestamp)
            .max()
            .unwrap();
        assert_eq!((newest - base).num_milliseconds(), 1100);
        let mut sensor = DemoSensor::with_base(Duration::ZERO, base);
        let first = sensor.poll();
        assert_eq!(first.len(), 7);
        assert_eq!(first[0].ts, base);
        assert_eq!((first[6].ts - base).num_milliseconds(), 2843);
        sensor.reset();
        assert_eq!(sensor.poll()[0].ts, base, "reset keeps the base");
    }

    #[test]
    fn discover_twice_is_idempotent() {
        let mut g = graph();
        let report = DemoProvider.discover(&mut g).unwrap();
        assert_eq!(report.entities, 10);
        assert_eq!(report.edges, 12);
        assert_eq!(g.entity_count(), 10);
        assert_eq!(g.edge_count(), 12);
        assert_eq!(g.entity(AWS_ALB).unwrap().evidence.len(), 2);
    }

    #[test]
    fn events_table() {
        let base = Utc::now();
        let evs = events_from(base);
        assert_eq!(evs.len(), 7);
        let offsets: Vec<i64> = evs
            .iter()
            .map(|e| (e.ts - base).num_milliseconds())
            .collect();
        assert_eq!(offsets, vec![0, 120, 400, 1100, 2200, 2500, 2843]);
        assert_eq!(offsets[6] as u64, DEMO_DURATION_MS);
        assert_eq!(evs[0].source, EventSource::Ebpf);
        assert_eq!(evs[0].from, "web-frontend-7f9c8c:51542");
        assert_eq!(evs[0].to, "kube-proxy:15001");
        assert_eq!(evs[0].detail, "1.217ms");
        assert!(!evs[0].bidirectional);
        assert_eq!(evs[2].source, EventSource::Correlation);
        assert!(evs[2].bidirectional);
        assert_eq!(evs[2].arrow(), "↔");
        assert_eq!(evs[3].action, "accept");
        assert_eq!(evs[3].from, "52.95.23.10:443");
        assert_eq!(evs[4].detail, "(inferred)");
        assert_eq!(evs[6].source, EventSource::Discovery);
        assert_eq!(evs[6].action, "resolve");
        assert_eq!(evs[6].detail, "Kubernetes DNS");
        assert_eq!(events().len(), 7);
    }

    #[test]
    fn sensor_replays_one_per_interval() {
        // intervalo enorme: só o primeiro evento está disponível de imediato
        let mut s = DemoSensor::new(Duration::from_secs(3600));
        assert_eq!(s.name(), "demo");
        assert_eq!(s.remaining(), 7);
        let first = s.poll();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].action, "connect");
        assert!(s.poll().is_empty());
        assert_eq!(s.remaining(), 6);
        assert!(!s.is_finished());

        // intervalo zero: tudo de uma vez; reset recomeça
        let mut z = DemoSensor::new(Duration::ZERO);
        assert_eq!(z.poll().len(), 7);
        assert!(z.is_finished());
        assert!(z.poll().is_empty());
        z.reset();
        assert_eq!(z.remaining(), 7);
        assert_eq!(z.poll().len(), 7);
        assert_eq!(z.interval(), Duration::ZERO);

        // intervalo curto: depois de esperar, o resto chega
        let mut q = DemoSensor::new(Duration::from_millis(5));
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(q.poll().len(), 7);
    }

    #[test]
    fn request_and_environment() {
        let r = default_request();
        assert_eq!(r.from, "web-frontend-7f9c8c");
        assert_eq!(r.to, "api.prod.svc.cluster.local");
        assert_eq!(r.port, 443);
        assert_eq!(r.protocol, "HTTPS");
        let env = environment();
        assert_eq!(env.environment, "prod");
        assert_eq!(env.cluster, "prod-e1");
        assert_eq!(env.user, "admin");
        assert_eq!(env.endpoint, "peabiru.local:443");
        assert!(env.connected);
    }
}
