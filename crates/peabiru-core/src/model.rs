//! Modelo normalizado (VISION §11, §14): identidade, tempo, providers, tipos
//! de entidade e endereços. Nenhum tipo específico de provider vive aqui.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use serde::{Deserialize, Serialize};

use crate::evidence::{merge_evidence, observational_sources, Confidence, Evidence};
use crate::status::NodeStatus;

/// Identificador URI-like de uma entidade:
/// `k8s://prod/pod/web-frontend-7f9c8c`, `aws://123456789012/alb/alb2c3`, `ip://10.0.1.5`.
pub type EntityId = String;

/// Instante em UTC.
pub type Timestamp = chrono::DateTime<chrono::Utc>;

/// Janela de validade temporal (VISION §14). `None` = ilimitado naquele lado.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Validity {
    pub valid_from: Option<Timestamp>,
    pub valid_until: Option<Timestamp>,
}

impl Validity {
    /// Validade ilimitada nos dois sentidos.
    pub fn always() -> Self {
        Self::default()
    }

    /// `true` se `t` está no intervalo semiaberto `[valid_from, valid_until)`.
    pub fn is_valid_at(&self, t: Timestamp) -> bool {
        self.valid_from.is_none_or(|from| from <= t)
            && self.valid_until.is_none_or(|until| t < until)
    }

    /// Merge: mantém o `valid_from` mais antigo e o `valid_until` mais recente.
    /// Um lado desconhecido (`None`) não sobrescreve um valor conhecido.
    pub(crate) fn merge(&mut self, other: &Validity) {
        self.valid_from = match (self.valid_from, other.valid_from) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        self.valid_until = match (self.valid_until, other.valid_until) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
    }
}

/// Origem (provider) de uma entidade ou evidência.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    Aws,
    Azure,
    Gcp,
    Magalu,
    Kubernetes,
    Linux,
    Vmware,
    Openstack,
    Network,
    Demo,
    Unknown,
}

impl Provider {
    /// Rótulo curto em minúsculas: `"aws"`, `"kubernetes"`, ...
    pub fn label(&self) -> &'static str {
        match self {
            Provider::Aws => "aws",
            Provider::Azure => "azure",
            Provider::Gcp => "gcp",
            Provider::Magalu => "magalu",
            Provider::Kubernetes => "kubernetes",
            Provider::Linux => "linux",
            Provider::Vmware => "vmware",
            Provider::Openstack => "openstack",
            Provider::Network => "network",
            Provider::Demo => "demo",
            Provider::Unknown => "unknown",
        }
    }
}

/// Tipo genérico de entidade (camada de normalização, VISION §11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityKind {
    Process,
    Host,
    Interface,
    Ip,
    Network,
    Subnet,
    Route,
    Gateway,
    Firewall,
    LoadBalancer,
    Nat,
    Tunnel,
    Endpoint,
    Cluster,
    Node,
    Service,
    Workload,
    Pod,
    Proxy,
    Database,
    Cache,
    Unknown,
}

impl EntityKind {
    /// Rótulo em `snake_case`: `"load_balancer"`, `"pod"`, ...
    pub fn label(&self) -> &'static str {
        match self {
            EntityKind::Process => "process",
            EntityKind::Host => "host",
            EntityKind::Interface => "interface",
            EntityKind::Ip => "ip",
            EntityKind::Network => "network",
            EntityKind::Subnet => "subnet",
            EntityKind::Route => "route",
            EntityKind::Gateway => "gateway",
            EntityKind::Firewall => "firewall",
            EntityKind::LoadBalancer => "load_balancer",
            EntityKind::Nat => "nat",
            EntityKind::Tunnel => "tunnel",
            EntityKind::Endpoint => "endpoint",
            EntityKind::Cluster => "cluster",
            EntityKind::Node => "node",
            EntityKind::Service => "service",
            EntityKind::Workload => "workload",
            EntityKind::Pod => "pod",
            EntityKind::Proxy => "proxy",
            EntityKind::Database => "database",
            EntityKind::Cache => "cache",
            EntityKind::Unknown => "unknown",
        }
    }
}

/// Endereço de rede: IP e/ou hostname, com porta opcional.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address {
    pub ip: Option<IpAddr>,
    pub port: Option<u16>,
    pub hostname: Option<String>,
}

impl Address {
    /// Só IP.
    pub fn ip(ip: IpAddr) -> Self {
        Self {
            ip: Some(ip),
            ..Self::default()
        }
    }

    /// IP + porta.
    pub fn socket(ip: IpAddr, port: u16) -> Self {
        Self {
            ip: Some(ip),
            port: Some(port),
            hostname: None,
        }
    }

    /// Só hostname.
    pub fn host(h: &str) -> Self {
        Self {
            hostname: Some(h.to_string()),
            ..Self::default()
        }
    }

    /// RFC1918 / CGNAT 100.64/10 / loopback / link-local (v4) e ULA / `::1` /
    /// link-local (v6); IPv6 mapeado de v4 (`::ffff:a.b.c.d`) segue o v4 embutido
    /// (contrato em ARCHITECTURE §2.2). Endereço só com hostname → `false`.
    pub fn is_private(&self) -> bool {
        self.ip.is_some_and(is_private_ip)
    }

    /// `"10.0.2.34:443"` | `"alb2c3.elb.amazonaws.com"` | `"10.0.1.5"` | `"[::1]:443"`.
    /// Sem IP nem hostname → `""`.
    pub fn display(&self) -> String {
        match (self.ip, self.port, self.hostname.as_deref()) {
            (Some(ip), Some(port), _) => SocketAddr::new(ip, port).to_string(),
            (Some(ip), None, _) => ip.to_string(),
            (None, Some(port), Some(h)) => format!("{h}:{port}"),
            (None, None, Some(h)) => h.to_string(),
            (None, _, None) => String::new(),
        }
    }
}

/// `true` para IPv4 RFC1918 (10/8, 172.16/12, 192.168/16), 127/8, 169.254/16,
/// 100.64/10 (CGNAT) e IPv6 `fc00::/7`, `::1`, `fe80::/10`. IPv6 mapeado de v4
/// (`::ffff:a.b.c.d`) segue a regra do v4 embutido.
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_private_v4(v4),
            None => is_private_v6(v6),
        },
    }
}

fn is_private_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    let cgnat = o[0] == 100 && (o[1] & 0xc0) == 64;
    ip.is_private() || ip.is_loopback() || ip.is_link_local() || cgnat
}

fn is_private_v6(ip: Ipv6Addr) -> bool {
    let s0 = ip.segments()[0];
    let unique_local = (s0 & 0xfe00) == 0xfc00;
    let link_local = (s0 & 0xffc0) == 0xfe80;
    ip.is_loopback() || unique_local || link_local
}

/// Nó do grafo: qualquer coisa endereçável ou topológica (pod, ALB, NAT, ...).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    /// Nome curto de exibição: `"aws-alb"`.
    pub name: String,
    pub kind: EntityKind,
    pub provider: Provider,
    /// Primeira = principal.
    pub addresses: Vec<Address>,
    /// `"region"="us-east-1"`, `"as"="AS16509 (Amazon.com, Inc.)"`, `"type"="load_balancer (aws_alb)"`.
    pub attrs: BTreeMap<String, String>,
    /// Evidência de que a entidade existe / foi vista.
    pub evidence: Vec<Evidence>,
    pub validity: Validity,
}

impl Entity {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        kind: EntityKind,
        provider: Provider,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            provider,
            addresses: Vec::new(),
            attrs: BTreeMap::new(),
            evidence: Vec::new(),
            validity: Validity::always(),
        }
    }

    /// Acrescenta um endereço (ignorado se idêntico a um já presente).
    pub fn with_address(mut self, a: Address) -> Self {
        if !self.addresses.contains(&a) {
            self.addresses.push(a);
        }
        self
    }

    pub fn with_attr(mut self, k: &str, v: &str) -> Self {
        self.attrs.insert(k.to_string(), v.to_string());
        self
    }

    pub fn with_evidence(mut self, e: Evidence) -> Self {
        self.evidence.push(e);
        self
    }

    pub fn primary_address(&self) -> Option<&Address> {
        self.addresses.first()
    }

    /// Status derivado (ARCHITECTURE §2.4):
    /// 1. `attrs["probe"] == "true"` → `Probe`;
    /// 2. ≥ 2 fontes observacionais distintas → `Correlated`;
    /// 3. ≥ 1 → `Observed`;
    /// 4. senão → `Inferred`.
    pub fn status(&self) -> NodeStatus {
        if self.attrs.get("probe").is_some_and(|v| v == "true") {
            return NodeStatus::Probe;
        }
        NodeStatus::from_evidence(&self.evidence)
    }

    /// Fontes distintas de evidência observacional, na ordem em que aparecem
    /// (para `SOURCE: 2 sensors`).
    pub fn sources(&self) -> Vec<String> {
        observational_sources(&self.evidence)
    }

    /// Máximo da confiança declarada nas evidências (0 se nenhuma).
    pub fn confidence(&self) -> Confidence {
        self.evidence
            .iter()
            .map(|e| e.confidence)
            .max()
            .unwrap_or_default()
    }

    /// Merge usado por `Graph::upsert_entity`: nome existente é mantido salvo se
    /// vazio; `kind`/`provider` `Unknown` são promovidos; endereços novos são
    /// anexados; attrs novos inseridos e existentes sobrescritos; evidência
    /// mesclada sem duplicar `(kind, source)`; validade unida.
    pub(crate) fn merge(&mut self, other: Entity) {
        if self.name.is_empty() {
            self.name = other.name;
        }
        if self.kind == EntityKind::Unknown {
            self.kind = other.kind;
        }
        if self.provider == Provider::Unknown {
            self.provider = other.provider;
        }
        for a in other.addresses {
            if !self.addresses.contains(&a) {
                self.addresses.push(a);
            }
        }
        self.attrs.extend(other.attrs);
        merge_evidence(&mut self.evidence, other.evidence);
        self.validity.merge(&other.validity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::EvidenceKind;
    use chrono::{TimeZone, Utc};

    fn v4(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn ev(kind: EvidenceKind, source: &str) -> Evidence {
        Evidence::new(kind, source, Provider::Demo, Confidence::new(90))
    }

    #[test]
    fn is_private_ipv4() {
        for ip in [
            "10.0.1.5",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "127.0.0.1",
            "169.254.10.10",
            "100.64.0.1",
            "100.127.255.255",
        ] {
            assert!(Address::ip(v4(ip)).is_private(), "{ip} should be private");
        }
        for ip in [
            "52.95.23.10",
            "8.8.8.8",
            "172.32.0.1",
            "100.128.0.1",
            "100.63.255.255",
        ] {
            assert!(!Address::ip(v4(ip)).is_private(), "{ip} should be public");
        }
    }

    #[test]
    fn is_private_ipv6() {
        for ip in [
            "::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "febf::1",
            "::ffff:10.0.0.1",
        ] {
            assert!(Address::ip(v4(ip)).is_private(), "{ip} should be private");
        }
        for ip in ["2001:db8::1", "fec0::1", "::ffff:8.8.8.8", "fb00::1"] {
            assert!(!Address::ip(v4(ip)).is_private(), "{ip} should be public");
        }
    }

    /// Pina o contrato de ARCHITECTURE §2.2: CGNAT conta como privado e o IPv6
    /// mapeado de v4 é classificado pelo v4 embutido (a regra de tromboning do
    /// `rules/default.lua` depende disso). O formato deprecado `::a.b.c.d`
    /// (IPv4-compatible) não é desembrulhado.
    #[test]
    fn is_private_cgnat_and_v4_mapped_follow_contract() {
        for ip in [
            "100.64.0.0",
            "100.127.255.255",
            "::ffff:100.64.0.1",
            "::ffff:192.168.0.1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.0.1",
        ] {
            assert!(is_private_ip(v4(ip)), "{ip} should be private");
        }
        for ip in [
            "100.63.255.255",
            "100.128.0.0",
            "::ffff:100.128.0.1",
            "::ffff:1.1.1.1",
            "::10.0.0.1",
        ] {
            assert!(!is_private_ip(v4(ip)), "{ip} should be public");
        }
    }

    #[test]
    fn is_private_hostname_only() {
        assert!(!Address::host("alb2c3.elb.amazonaws.com").is_private());
        assert!(!Address::default().is_private());
    }

    #[test]
    fn address_display() {
        assert_eq!(
            Address::socket(v4("10.0.2.34"), 443).display(),
            "10.0.2.34:443"
        );
        assert_eq!(Address::ip(v4("10.0.1.5")).display(), "10.0.1.5");
        assert_eq!(Address::socket(v4("::1"), 443).display(), "[::1]:443");
        assert_eq!(
            Address::host("alb2c3.elb.amazonaws.com").display(),
            "alb2c3.elb.amazonaws.com"
        );
        let host_port = Address {
            ip: None,
            port: Some(8080),
            hostname: Some("svc".into()),
        };
        assert_eq!(host_port.display(), "svc:8080");
        assert_eq!(Address::default().display(), "");
    }

    #[test]
    fn validity_window() {
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 1, 3, 0, 0, 0).unwrap();
        let v = Validity {
            valid_from: Some(t0),
            valid_until: Some(t1),
        };
        assert!(v.is_valid_at(t0));
        assert!(!v.is_valid_at(t1));
        assert!(!v.is_valid_at(t2));
        assert!(Validity::always().is_valid_at(t2));

        let mut a = Validity {
            valid_from: Some(t1),
            valid_until: None,
        };
        a.merge(&Validity {
            valid_from: Some(t0),
            valid_until: Some(t2),
        });
        assert_eq!(a.valid_from, Some(t0));
        assert_eq!(a.valid_until, Some(t2));
        a.merge(&Validity::always());
        assert_eq!(a.valid_from, Some(t0), "None must not erase a known bound");
        assert_eq!(a.valid_until, Some(t2));
    }

    #[test]
    fn status_inferred_without_observational_evidence() {
        let e = Entity::new(
            "aws://1/natgw/nat-0a1b",
            "nat-gw",
            EntityKind::Nat,
            Provider::Aws,
        )
        .with_evidence(ev(EvidenceKind::Discovered, "AWS Route Table"));
        assert_eq!(e.status(), NodeStatus::Inferred);
        assert!(e.sources().is_empty());
        let bare = Entity::new(
            "ip://10.0.3.8",
            "redis",
            EntityKind::Cache,
            Provider::Unknown,
        );
        assert_eq!(bare.status(), NodeStatus::Inferred);
        assert_eq!(bare.confidence(), Confidence(0));
    }

    #[test]
    fn status_observed_with_one_source() {
        let e = Entity::new(
            "k8s://prod/service/app-svc",
            "app-svc",
            EntityKind::Service,
            Provider::Kubernetes,
        )
        .with_evidence(ev(EvidenceKind::Observed, "eBPF (node/ip-10-42-1-1)"))
        .with_evidence(ev(EvidenceKind::Observed, "eBPF (node/ip-10-42-1-1)"))
        .with_evidence(ev(EvidenceKind::Discovered, "Kubernetes API"));
        assert_eq!(e.status(), NodeStatus::Observed);
        assert_eq!(e.sources(), vec!["eBPF (node/ip-10-42-1-1)".to_string()]);
    }

    #[test]
    fn status_correlated_with_two_sources() {
        let e = Entity::new(
            "aws://1/alb/alb2c3",
            "aws-alb",
            EntityKind::LoadBalancer,
            Provider::Aws,
        )
        .with_evidence(ev(EvidenceKind::Observed, "eBPF (node/ip-10-0-1-25)"))
        .with_evidence(ev(EvidenceKind::LogConfirmed, "VPC Flow Logs"));
        assert_eq!(e.status(), NodeStatus::Correlated);
        assert_eq!(e.sources().len(), 2);
        assert_eq!(e.confidence(), Confidence(90));
    }

    #[test]
    fn status_probe_overrides_evidence() {
        let e = Entity::new("probe://1", "probe-1", EntityKind::Host, Provider::Demo)
            .with_attr("probe", "true")
            .with_evidence(ev(EvidenceKind::Observed, "a"))
            .with_evidence(ev(EvidenceKind::Observed, "b"));
        assert_eq!(e.status(), NodeStatus::Probe);
        let not_probe = e.clone().with_attr("probe", "false");
        assert_eq!(not_probe.status(), NodeStatus::Correlated);
    }

    #[test]
    fn builders_and_primary_address() {
        let e = Entity::new("ip://10.0.1.5", "nat-gw", EntityKind::Nat, Provider::Aws)
            .with_address(Address::ip(v4("10.0.1.5")))
            .with_address(Address::ip(v4("10.0.1.5")))
            .with_address(Address::host("nat.internal"))
            .with_attr("region", "us-east-1");
        assert_eq!(e.addresses.len(), 2);
        assert_eq!(
            e.primary_address().map(Address::display).as_deref(),
            Some("10.0.1.5")
        );
        assert_eq!(e.attrs.get("region").map(String::as_str), Some("us-east-1"));
        assert_eq!(EntityKind::LoadBalancer.label(), "load_balancer");
        assert_eq!(Provider::Kubernetes.label(), "kubernetes");
    }
}
