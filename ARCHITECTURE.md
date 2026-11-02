# Peabiru: Arquitetura da base (v0.1)

Este documento é o **contrato** entre os crates. Quem implementa um crate lê
este arquivo primeiro; quem altera uma interface pública atualiza este arquivo
no mesmo commit. A visão conceitual completa está em `docs/VISION.md`.

Princípios que a base já respeita (ver VISION §29):

- **Graph-first**: tudo gira em torno do `Graph` em `peabiru-core`.
- **Core agnóstico de provider**: nenhum tipo AWS/K8s dentro de `peabiru-core`.
- **Evidence e Confidence são first-class**: toda aresta carrega `Vec<Evidence>`;
  nada inferido é exibido como observado.
- **Lua fora do datapath**: Lua só entra em diagnósticos (regras) e em
  customização da UI (tema). Nunca em captura/agregação.
- **Sem shell-out**: nenhum `Command::new("tcpdump"|"kubectl"|"aws")`.

## 1. Workspace

```
peabiru/
├── Cargo.toml                     # workspace
├── ARCHITECTURE.md                # este contrato
├── docs/VISION.md                 # visão conceitual (origem do projeto)
├── rules/default.lua              # regras de diagnóstico padrão (Lua)
└── crates/
    ├── peabiru-core/              # modelo, grafo temporal, evidência, eventos
    ├── peabiru-trace/             # trace engine: path finding, confiança, correlações
    ├── peabiru-discovery/         # trait Provider/Sensor + provider "demo"
    ├── peabiru-lua/               # runtime Lua: regras de diagnóstico + config/tema
    ├── peabiru-tui/               # interface ratatui (visual dos mockups)
    └── peabiru-cli/               # binário `peabiru`
```

Grafo de dependências (setas = "depende de"):

```
cli ──► tui ──► trace ──► core
 │       └──► discovery ──► core, trace
 ├──► lua ──► core, trace
 └──► discovery
```

`peabiru-core` não depende de nenhum outro crate do workspace nem de
ratatui/mlua/clap. `peabiru-tui` não depende de `peabiru-lua`: os findings
das regras chegam por `App.findings`, preenchido pelo CLI.

## 2. peabiru-core

### 2.1 Identidade e tempo

```rust
pub type EntityId = String;            // URI-like: "k8s://prod/pod/web-frontend-7f9c8c",
                                       //           "aws://123456789012/alb/alb2c3", "ip://10.0.1.5"
pub type Timestamp = chrono::DateTime<chrono::Utc>;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Validity { pub valid_from: Option<Timestamp>, pub valid_until: Option<Timestamp> }
impl Validity { pub fn is_valid_at(&self, t: Timestamp) -> bool; pub fn always() -> Self; }
```

### 2.2 Entidades (modelo normalizado, VISION §11)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider { Aws, Azure, Gcp, Magalu, Kubernetes, Linux, Vmware, Openstack, Network, Demo, Unknown }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityKind {
    Process, Host, Interface, Ip, Network, Subnet, Route, Gateway, Firewall,
    LoadBalancer, Nat, Tunnel, Endpoint, Cluster, Node, Service, Workload,
    Pod, Proxy, Database, Cache, Unknown,
}
impl EntityKind { pub fn label(&self) -> &'static str; /* "load_balancer", "pod", ... */ }

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Address { pub ip: Option<std::net::IpAddr>, pub port: Option<u16>, pub hostname: Option<String> }
impl Address {
    pub fn ip(ip: IpAddr) -> Self; pub fn socket(ip: IpAddr, port: u16) -> Self; pub fn host(h: &str) -> Self;
    pub fn is_private(&self) -> bool;           // v4: RFC1918 / CGNAT 100.64/10 / loopback / link-local;
                                                // v6: ULA fc00::/7 / ::1 / fe80::/10; `::ffff:a.b.c.d` segue o v4 embutido
    pub fn display(&self) -> String;            // "10.0.2.34:443" | "alb2c3.elb.amazonaws.com" | "10.0.1.5"
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub name: String,                           // nome curto de exibição: "aws-alb"
    pub kind: EntityKind,
    pub provider: Provider,
    pub addresses: Vec<Address>,                // primeira = principal
    pub attrs: BTreeMap<String, String>,        // "region"="us-east-1", "as"="AS16509 (Amazon.com, Inc.)", "type"="load_balancer (aws_alb)"
    pub evidence: Vec<Evidence>,                // evidência de que a entidade existe / foi vista
    pub validity: Validity,
}
impl Entity {
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: EntityKind, provider: Provider) -> Self;
    pub fn with_address(self, a: Address) -> Self;
    pub fn with_attr(self, k: &str, v: &str) -> Self;
    pub fn with_evidence(self, e: Evidence) -> Self;
    pub fn primary_address(&self) -> Option<&Address>;
    pub fn status(&self) -> NodeStatus;         // ver 2.4
    pub fn sources(&self) -> Vec<String>;       // fontes distintas de evidência observacional (para "SOURCE: 2 sensors")
    pub fn confidence(&self) -> Confidence;     // max das evidências (0 se nenhuma)
}
```

### 2.3 Evidência e confiança (VISION §15–16)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EvidenceKind { Observed, Inferred, ActiveTest, LogConfirmed, Discovered }
impl EvidenceKind {
    pub fn label(&self) -> &'static str;        // "OBSERVED", "INFERRED", "ACTIVE_TEST", "LOG_CONFIRMED", "DISCOVERED"
    pub fn is_observational(&self) -> bool;     // Observed | ActiveTest | LogConfirmed => true
    pub fn weight(&self) -> f64;                // Observed 1.0, LogConfirmed 0.95, ActiveTest 0.9, Discovered 0.85, Inferred 0.7
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Confidence(pub u8);                  // 0..=100 (%)
impl Confidence {
    pub const VERIFIED: Confidence = Confidence(100);
    pub fn new(pct: u8) -> Self;                // clamp em 100
    pub fn from_ratio(r: f64) -> Self;
    pub fn pct(&self) -> u8;
    pub fn ratio(&self) -> f64;
    pub fn label(&self) -> &'static str;        // >=95 "VERIFIED", >=80 "HIGH", >=60 "MEDIUM", else "LOW"
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub source: String,                         // "eBPF (node/ip-10-0-1-25)", "VPC Flow Logs", "AWS Route Table", "Kubernetes API"
    pub provider: Provider,
    pub timestamp: Timestamp,
    pub confidence: Confidence,
    pub metadata: BTreeMap<String, String>,
}
impl Evidence {
    pub fn new(kind: EvidenceKind, source: &str, provider: Provider, confidence: Confidence) -> Self; // timestamp = Utc::now()
    pub fn at(self, t: Timestamp) -> Self;
    pub fn with_meta(self, k: &str, v: &str) -> Self;
    pub fn effective_confidence(&self) -> f64;  // kind.weight() * confidence.ratio()
}
```

### 2.4 Status derivado (o que a UI desenha)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeStatus { Observed, Inferred, Correlated, Probe }
impl NodeStatus {
    pub fn label(&self) -> &'static str;        // "OBSERVED" | "INFERRED" | "CORRELATED" | "PROBE"
    pub fn is_observed(&self) -> bool;          // Observed | Correlated | Probe
}
```

Regra de derivação em `Entity::status()`:

1. Se `attrs["probe"] == "true"` → `Probe`.
2. Fontes observacionais distintas (`evidence` com `kind.is_observational()`, dedup por `source`) ≥ 2 → `Correlated`.
3. ≥ 1 fonte observacional → `Observed`.
4. Caso contrário → `Inferred`.

### 2.5 Arestas e grafo

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Relation { Connects, RoutesTo, ForwardsTo, Contains, ResolvesTo, Correlated, Peer }
impl Relation { pub fn label(&self) -> &'static str; pub fn arrow(&self) -> &'static str; /* "→" | "↔" */ }

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Metrics { pub rtt_ms: Option<f64>, pub retransmissions: u32, pub drops: u32, pub rst: u32 }

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
    pub fn new(from: &str, to: &str, relation: Relation) -> Self;
    pub fn with_evidence(self, e: Evidence) -> Self;
    pub fn with_metrics(self, m: Metrics) -> Self;
    pub fn status(&self) -> NodeStatus;         // mesma regra da entidade, sobre evidence da aresta (sem Probe)
    pub fn confidence(&self) -> Confidence;     // ver peabiru-trace::confidence::edge_confidence; core expõe max(effective_confidence)
    pub fn best_evidence(&self) -> Option<&Evidence>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeId(pub usize);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Graph { /* entities: IndexMap<EntityId, Entity>, edges: Vec<Edge>, out/in adjacency */ }
impl Graph {
    pub fn new() -> Self;
    pub fn upsert_entity(&mut self, e: Entity) -> &mut Entity;  // merge: addresses, attrs, evidence (append, sem duplicar source+kind)
    pub fn add_edge(&mut self, e: Edge) -> EdgeId;              // dedup por (from,to,relation): merge evidence/metrics
    pub fn entity(&self, id: &str) -> Option<&Entity>;
    pub fn entity_mut(&mut self, id: &str) -> Option<&mut Entity>;
    pub fn entities(&self) -> impl Iterator<Item = &Entity>;
    pub fn edges(&self) -> impl Iterator<Item = (EdgeId, &Edge)>;
    pub fn edge(&self, id: EdgeId) -> Option<&Edge>;
    pub fn out_edges(&self, id: &str) -> Vec<(EdgeId, &Edge)>;
    pub fn in_edges(&self, id: &str) -> Vec<(EdgeId, &Edge)>;
    pub fn neighbors(&self, id: &str) -> Vec<(EdgeId, &Edge, &Entity)>;  // ambas direções
    pub fn find_by_address(&self, ip: IpAddr, port: Option<u16>) -> Vec<&Entity>;
    pub fn find_by_name(&self, name: &str) -> Option<&Entity>;
    pub fn resolve(&self, selector: &str) -> Option<&Entity>;  // id exato | name | "ip" | "ip:port" | hostname
    pub fn entity_count(&self) -> usize; pub fn edge_count(&self) -> usize;
}
```

### 2.6 Eventos e findings

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventSource { Ebpf, FlowLog, Correlation, Inference, Probe, Discovery }
impl EventSource { pub fn label(&self) -> &'static str; /* "eBPF" "flow_log" "correlation" "inference" "probe" "discovery" */
                   pub fn status(&self) -> NodeStatus; /* Ebpf/FlowLog/Probe→Observed, Correlation→Correlated, Inference/Discovery→Inferred */ }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub ts: Timestamp,
    pub source: EventSource,
    pub action: String,      // "connect" | "match" | "accept" | "route" | "probe"
    pub from: String,        // texto já formatado: "web-frontend-7f9c8c:51542"
    pub to: String,          // "kube-proxy:15001"
    pub bidirectional: bool, // true → seta "↔"
    pub detail: String,      // "1.217ms" | "92%" | "(inferred)"
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Severity { Info, Warning, Critical }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding { pub severity: Severity, pub message: String, pub rule: String, pub subject: Option<EntityId> }
```

Módulos: `model.rs` (Provider, EntityKind, Address, Entity, Validity, Timestamp),
`evidence.rs` (EvidenceKind, Confidence, Evidence), `status.rs` (NodeStatus),
`graph.rs` (Relation, Metrics, Edge, EdgeId, Graph), `events.rs` (EventSource,
Event), `diagnostics.rs` (Severity, Finding). `lib.rs` re-exporta tudo (`pub use`).
Testes unitários em cada módulo (merge de entidade, dedup de aresta, status,
is_private, resolve).

Extras aditivos implementados em `peabiru-core` (além das assinaturas acima):
`Provider::label()` (`"aws"`, `"kubernetes"`, ...); `Severity::label()`,
`Display` e `FromStr` (case-insensitive; erro `ParseSeverityError` via thiserror),
`Severity: PartialOrd + Ord` (Info < Warning < Critical); `Finding::new(severity,
rule, message)` / `with_subject(id)`; `NodeStatus::from_evidence(&[Evidence])`
(regras 2–4 de §2.4, sem `Probe`); `Confidence: Display` (`"92%"`);
`observational_sources(&[Evidence]) -> Vec<String>`; `is_private_ip(IpAddr)`;
`Address: Eq + Hash`; `Event::arrow()`; `Graph: PartialEq` (entidades + arestas)
e `Graph::from_parts(IndexMap<EntityId, Entity>, Vec<Edge>)`. Semântica de merge:
`Validity` mantém o `valid_from` mais antigo / `valid_until` mais recente e `None`
não sobrescreve valor conhecido; evidência com mesmo `(kind, source)` não é
duplicada: a entrada existente recebe timestamp/confiança da mais recente e os
metadados são mesclados; `Metrics` prefere `Some(rtt)` (máx. se ambos) e máximo
dos contadores; `kind`/`provider` `Unknown` são promovidos no upsert.
`Edge::confidence()` é 0 sem evidência (o fallback 0.5 fica em `peabiru-trace`).

## 3. peabiru-trace

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceRequest {
    pub from: String,        // selector (Graph::resolve)
    pub to: String,          // selector ou FQDN alvo: "api.prod.svc.cluster.local"
    pub port: u16,
    pub protocol: String,    // "HTTPS" | "TCP" | "HTTP" (só rótulo por enquanto)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceStatus { Idle, Running, Completed, Failed }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hop {
    pub index: usize,
    pub entity: EntityId,
    pub name: String,
    pub kind: EntityKind,
    pub status: NodeStatus,
    pub address: String,           // Address::display() da principal ("" se nenhuma)
    pub edge_in: Option<EdgeId>,   // aresta que chega neste hop (None no primeiro)
    pub edge_status: NodeStatus,   // status da aresta de entrada (Observed → linha sólida, Inferred → tracejada)
    pub edge_source: Option<String>, // `source` da melhor evidência da aresta de entrada (None no primeiro / sem evidência); #[serde(default)]
    pub confidence: Confidence,    // da aresta de entrada (100 no primeiro)
    pub metrics: Metrics,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Branch {                // nó vizinho fora do caminho principal (nat-gw, redis, orders-db, payments-svc)
    pub anchor: usize,             // índice do Hop ao qual está ligado
    pub entity: EntityId,
    pub name: String,
    pub kind: EntityKind,
    pub status: NodeStatus,
    pub address: String,
    pub edge: EdgeId,
    pub edge_status: NodeStatus,
    pub direction: Direction,      // Outgoing (hop→branch) | Incoming (branch→hop)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] pub enum Direction { Outgoing, Incoming }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Correlation { pub a: EntityId, pub a_name: String, pub b: EntityId, pub b_name: String, pub score: Confidence, pub basis: String }

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary { pub hops: usize, pub observed: usize, pub inferred: usize, pub correlations: usize, pub confidence: Confidence, pub duration_ms: u64 }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceResult {
    pub id: u32,
    pub status: TraceStatus,
    pub request: TraceRequest,
    pub started_at: Timestamp,
    pub hops: Vec<Hop>,            // caminho principal, ordem from→to
    pub branches: Vec<Branch>,
    pub correlations: Vec<Correlation>,
    pub summary: Summary,
    pub findings: Vec<Finding>,    // preenchido por peabiru-lua (vazio se sem regras)
}
impl TraceResult {
    pub fn hop_for(&self, id: &str) -> Option<&Hop>;
    pub fn render_ascii(&self) -> String;   // saída texto do VISION §24 (para `peabiru trace` sem --json)
}

pub struct TraceEngine;
impl TraceEngine {
    pub fn trace(graph: &Graph, req: &TraceRequest) -> Result<TraceResult, TraceError>;
}
#[derive(Debug, thiserror::Error)] pub enum TraceError { #[error("source not found: {0}")] SourceNotFound(String), #[error("target not found: {0}")] TargetNotFound(String), #[error("no path from {from} to {to}")] NoPath { from: String, to: String } }
```

Algoritmo (`path.rs`): Dijkstra sobre arestas **de saída** (Connects, RoutesTo,
ForwardsTo, ResolvesTo, Peer; ignora Contains/Correlated), custo da aresta
`1.0 + (1.0 - edge_confidence)`, que prefere caminhos observados com o mesmo
número de hops. `branches` = para cada hop, vizinhos (in+out) que não estão no
caminho, via qualquer relação exceto Contains/Correlated.

`confidence.rs`: `edge_confidence(&Edge) -> f64` = máx de
`effective_confidence()` das evidências (0.5 se sem evidência);
`path_confidence(&[Hop]) -> Confidence` = média geométrica das confianças das
arestas de entrada (hop 0 excluído), arredondada. `summary.observed` conta hops
com `status.is_observed()`, `inferred` o resto.

`correlation.rs`: coleta arestas `Relation::Correlated` cujo `from`/`to` estão no
caminho ou nas branches; `score` = `edge.confidence()`, `basis` =
`correlation_basis(&edge)`: `metadata["basis"]` da melhor evidência (ou, na
falta, da primeira evidência que o tenha), senão as fontes distintas das
evidências unidas por `" + "` (`""` sem evidência). A aresta guarda as
evidências **reais** que a correlação junta; o `basis` é só o rótulo.

Precisões implementadas em `peabiru-trace` (aditivas ao contrato acima):

- **`Hop.edge_source`** (campo extra, `#[serde(default)]`): `source` da melhor
  evidência da aresta de entrada, porque `render_ascii()` recebe só `&self`, sem
  grafo, e precisa imprimir a linha `│ <fonte>` do VISION §24. Hop 0 sai com
  `edge_in = None`, `edge_status = Observed`, `edge_source = None`,
  `confidence = 100`, `metrics = default`.
- **Destino** (`path::resolve_target` → `TraceTarget { selected, entity,
  resolution }`): `Graph::resolve(to)` dá `selected`; segue-se `ResolvesTo` de
  saída (primeira aresta em ordem de inserção, alvo existente, sem repetir
  nó) repetidamente (cadeias CNAME) até `entity`. Se houve desembrulho, a
  entidade `selected` (ex.: `dns://api.prod.svc.cluster.local`) **nunca**
  aparece como branch; `TraceRequest.to` continua com o FQDN.
- **Dijkstra** (`path::shortest_path`): custos em micro-unidades inteiras;
  nós assentados por `(custo, ordem de descoberta)` e relaxação só se
  estritamente melhor: entre caminhos de custo igual vence o descoberto
  primeiro (arestas inseridas antes). Arestas para entidades ausentes do grafo
  são ignoradas. `from == to` → caminho de 1 hop, confiança 100.
- **Branches** (`path::collect_branches`): ordem = hops, e dentro de cada hop a
  de `Graph::neighbors` (saída antes de entrada); cada entidade aparece **uma**
  vez (na primeira âncora em que é vista).
- **Correlações**: exigem os **dois** extremos em `hops ∪ branches` (para a UI
  conseguir selecionar `a`); ordem = inserção no grafo; `basis = ""` sem evidência.
  `correlation::BASIS_META` (`"basis"`) é a chave de metadata lida por
  `correlation_basis`.
- **`summary.duration_ms`** = janela de observação: ms entre a evidência mais
  antiga e a mais recente entre entidades dos hops, arestas de entrada, arestas
  das branches, arestas de correlação e arestas `ResolvesTo` seguidas (0 sem
  evidência). No demo dá 2843 porque a fixture carimba as evidências em
  `agora + offset` da timeline de §4.1.
- **`TraceResult.id`**: contador por processo (`TraceEngine::next_id()`,
  começa em 1). `status` é sempre `Completed` no `Ok` (erros vão em `TraceError`).
- **`render_ascii()`**: `PEABIRU TRACE #<id>`, linha em branco; por hop `nome`
  + `endereço` (se houver) + linha em branco; entre hops o bloco `     │` /
  `     │ <EDGE_STATUS>` / `     │ <edge_source>` (se houver) / `     ▼` +
  linha em branco; depois `Diagnostics:` com `TCP RTT:` (máx dos hops, `n/a`
  se nenhum), `Retransmissions:`, `Kernel drops:`, `RST:` (somas), linha em
  branco, `Path confidence:     NN%`; se houver findings, `Potential problem:`
  e uma linha `[severity] mensagem` por finding. Rótulos alinhados em 21 colunas.
- Extras públicos: `TraceRequest::new(from, to, port, protocol)`,
  `TraceStatus::label()`, `TraceResult::{max_rtt_ms, total_retransmissions,
  total_drops, total_rst}`, `TraceTarget::was_unwrapped()`, `path::{PathStep,
  is_traversable, is_branch_relation, edge_cost, unwrap_resolves_to}`,
  `correlation::{correlation_edges, collect_correlations, correlation_basis, BASIS_META}`,
  `confidence::NO_EVIDENCE_CONFIDENCE` (0.5). `TraceError: Clone + PartialEq + Eq`.
  O teste de ponta a ponta sobre a fixture vive em
  `crates/peabiru-discovery/tests/demo_trace.rs` (evita ciclo de dev-dependency).

## 4. peabiru-discovery

```rust
pub trait Provider {
    fn name(&self) -> &'static str;                                  // "demo", "aws", "kubernetes"
    fn provider(&self) -> Provider;                                  // peabiru_core::Provider
    fn discover(&self, graph: &mut Graph) -> anyhow::Result<DiscoveryReport>;
}
#[derive(Clone, Debug, Default)] pub struct DiscoveryReport { pub entities: usize, pub edges: usize, pub warnings: Vec<String> }

pub trait Sensor {
    fn name(&self) -> &'static str;
    fn poll(&mut self) -> Vec<Event>;                                // eventos novos desde o último poll
}

pub mod demo {
    pub struct DemoProvider;                 // impl Provider: constrói a fixture dos mockups (abaixo)
    pub struct DemoSensor { .. }             // impl Sensor: replay dos eventos da fixture, um a cada `interval`
    impl DemoSensor { pub fn new(interval: std::time::Duration) -> Self; pub fn reset(&mut self);
                      pub fn with_base(interval: Duration, base: Timestamp) -> Self; // ts = base + offset (reset mantém a base)
                      pub fn interval(&self) -> Duration; pub fn remaining(&self) -> usize; pub fn is_finished(&self) -> bool; }
    pub fn default_request() -> TraceRequest; // from "web-frontend-7f9c8c", to "api.prod.svc.cluster.local", port 443, "HTTPS"
    pub fn events() -> Vec<Event>;            // timeline completa (abaixo) = events_from(Utc::now())
    pub fn events_from(base: Timestamp) -> Vec<Event>; // ts = base + offset da tabela
    pub fn graph() -> Graph;                  // Graph::new() já populado por DemoProvider (evidências em agora + offset)
    pub fn graph_from(base: Timestamp) -> Graph; // idem com evidências em base + offset (renders determinísticos)
    pub const DEMO_DURATION_MS: u64 = 2843;   // offset do último evento
    pub const WEB_FRONTEND: &str; KUBE_PROXY; ISTIO_INGRESS; NAT_GW; AWS_ALB; APP_SVC; REDIS; ORDERS_DB; PAYMENTS_SVC; API_DNS; // ids da fixture
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Environment { pub environment: String, pub cluster: String, pub user: String, pub endpoint: String, pub connected: bool }
    pub fn environment() -> Environment;      // "prod", "prod-e1", "admin", "peabiru.local:443", true
}
pub fn registry() -> Vec<Box<dyn Provider>>;  // por enquanto só DemoProvider
```

`demo::BASIS_META` (`"basis"`) é a chave de metadata das arestas `Correlated`
(mesmo valor de `peabiru_trace::correlation::BASIS_META`).

`DiscoveryReport.entities`/`edges` contam o que o provider **emitiu**
(`upsert_entity`/`add_edge`, mesmo que já existisse): o demo dá 10/12 e um
segundo `discover` no mesmo grafo é idempotente (merge do core).

`DemoSensor`: o relógio começa em `new()`/`reset()` (`std::time::Instant`); o
k-ésimo evento (0-based) fica disponível quando `elapsed >= k * interval`, ou
seja, o primeiro sai já no primeiro `poll` e os demais um por `interval`
(`interval` zero → todos de uma vez). Os `ts` dos eventos são os da timeline
(`início + offset`), não o instante do `poll`.

### 4.1 Fixture demo (reproduz os mockups)

Entidades (id, name, kind, provider, address, status esperado):

| id | name | kind | prov | address | status | extras |
|---|---|---|---|---|---|---|
| `k8s://prod/pod/web-frontend-7f9c8c` | web-frontend-7f9c8c | Pod | Kubernetes | 10.42.1.15:51542 | Observed | evid eBPF (node/ip-10-42-1-1) |
| `k8s://prod/node/kube-proxy` | kube-proxy | Proxy | Kubernetes | 10.42.1.1 | Observed | evid eBPF |
| `k8s://istio-system/pod/istio-ingressgateway` | istio-ingressgateway | Gateway | Kubernetes | 10.42.0.7:15001 | Observed | evid eBPF |
| `aws://123456789012/natgw/nat-0a1b` | nat-gw | Nat | Aws | 10.0.1.5 | Inferred | evid Discovered "AWS Route Table" |
| `aws://123456789012/alb/alb2c3` | aws-alb | LoadBalancer | Aws | alb2c3.elb.amazonaws.com + 52.95.23.10:443 | Correlated | attrs type="load_balancer (aws_alb)", as="AS16509 (Amazon.com, Inc.)", region="us-east-1", reachable="true"; evid Observed "eBPF (node/ip-10-0-1-25)" 92 + LogConfirmed "VPC Flow Logs" 90 |
| `k8s://prod/service/app-svc` | app-svc | Service | Kubernetes | 10.0.2.34:443 | Observed | evid eBPF + Discovered "Kubernetes API" |
| `ip://10.0.3.8` | redis | Cache | Unknown | 10.0.3.8:6379 | Inferred | evid Inferred "route inference" |
| `aws://123456789012/rds/orders-db` | orders-db | Database | Aws | 10.0.4.12:5432 | Correlated | evid LogConfirmed "VPC Flow Logs" + Observed "eBPF (node/ip-10-0-2-34)" |
| `k8s://prod/service/payments-svc` | payments-svc | Service | Kubernetes | 10.0.2.55:8080 | Inferred | evid Discovered "Kubernetes API" |
| `dns://api.prod.svc.cluster.local` | api.prod.svc.cluster.local | Endpoint | Kubernetes | hostname | Inferred | alvo lógico; `app-svc` ResolvesTo → ele **não**: o alvo resolve para `app-svc` (ver abaixo) |

Arestas do caminho principal (todas `Connects` salvo indicado):

1. web-frontend → kube-proxy: Observed eBPF, rtt 1.217
2. kube-proxy → istio-ingressgateway: Observed eBPF, rtt 0.892
3. istio-ingressgateway → aws-alb: Observed eBPF (node/ip-10-0-1-25) 92 + LogConfirmed VPC Flow Logs, rtt 1.103, **retransmissions 2** (RoutesTo). Única aresta com retransmissões; `total_retransmissions()` = 2 (VISION §24)
4. aws-alb → app-svc: LogConfirmed "VPC Flow Logs" (ForwardsTo), rtt 1.103
5. `dns://api.prod.svc.cluster.local` ResolvesTo app-svc: Discovered "Kubernetes DNS"; **e** app-svc é o último hop: o engine, ao receber `to = "api.prod.svc.cluster.local"`, resolve o selector para a entidade DNS e, se ela tiver aresta `ResolvesTo` de **saída** para outra entidade, usa essa entidade como destino real (registrar em `Hop`? Não, só o alvo muda; `TraceRequest.to` continua com o FQDN).

Branches: istio-ingressgateway → nat-gw (Inferred, RoutesTo, Outgoing);
app-svc → orders-db (Inferred "route inference" na aresta, mas entidade
Correlated); app-svc → redis (Inferred, Connects); app-svc → payments-svc
(Inferred). A ordem de inserção das arestas de saída do app-svc (orders-db,
redis, payments-svc) é a ordem das branches no `TraceResult` e no desenho
(orders-db sob o app-svc, redis à esquerda, payments-svc à direita).
Correlações (`Relation::Correlated`, status **Correlated**, nunca Observed):
aws-alb↔app-svc 92 ("VPC Flow Logs + eBPF 5-tuple"), web-frontend↔orders-db
81 ("cgroup + flow log"), istio-ingressgateway↔redis 73 ("timing correlation").

Eventos (`demo::events()`, ts relativos ao início, em ordem):

| +ms | source | action | from | to | bi | detail |
|---|---|---|---|---|---|---|
| 0 | Ebpf | connect | web-frontend-7f9c8c:51542 | kube-proxy:15001 | no | 1.217ms |
| 120 | Ebpf | connect | kube-proxy:15001 | istio-ingressgateway:15001 | no | 0.892ms |
| 400 | Correlation | match | istio-ingressgateway | aws-alb | yes | 92% |
| 1100 | FlowLog | accept | 52.95.23.10:443 | 10.0.2.34:443 | no | 1.103ms |
| 2200 | Inference | route | 10.0.2.34:443 | 10.0.4.12:5432 | no | (inferred) |
| 2500 | Inference | route | 10.0.2.34:443 | 10.0.3.8:6379 | no | (inferred) |
| 2843 | Discovery | resolve | api.prod.svc.cluster.local | app-svc | no | Kubernetes DNS |

`summary.duration_ms` do demo = 2843.

Valores que as tabelas deixam em aberto e a fixture fixa (`demo.rs`): fontes
eBPF por nó: `eBPF (node/ip-10-42-1-1)` (web-frontend, kube-proxy e arestas
1–2), `eBPF (node/ip-10-42-0-1)` (istio), `eBPF (node/ip-10-0-2-34)` (app-svc,
orders-db); confianças: eBPF 95 (92 no ALB/aresta 3, 88 no orders-db), VPC
Flow Logs 90, Kubernetes API/DNS 90, AWS Route Table 80, `route inference`
60/65/55 (redis/orders-db/payments-svc); a aresta istio → nat-gw usa Discovered
"AWS Route Table". As arestas `Correlated` levam as **duas** evidências reais
que a correlação junta (fontes observacionais distintas ⇒ `status()` =
`Correlated` em `peabiru graph` e na Topology): uma `Observed` com confiança
igual ao score (peso 1.0 ⇒ `edge.confidence()` = 92/81/73) e a segunda com
confiança efetiva menor (aws-alb↔app-svc: eBPF (node/ip-10-0-2-34) 92 + VPC
Flow Logs 90; web-frontend↔orders-db: eBPF (node/ip-10-42-1-1) 81 + VPC Flow
Logs 80; istio↔redis: eBPF (node/ip-10-42-0-1) 73 + eBPF (node/ip-10-0-2-34)
70). O texto do basis vai em `metadata["basis"]` (`demo::BASIS_META`) de ambas
as evidências. Toda evidência é carimbada em `agora + offset` seguindo a timeline
(0 … 2843 ms), o que dá `duration_ms = 2843`. Attrs extras: `namespace`,
`node`, `region`, `type`, `reachable` (app-svc também `reachable="true"`).
Resultado do trace demo: confiança 92%, 5 hops observados / 0 inferidos.

## 5. peabiru-lua

```rust
pub struct RuleEngine { .. }
impl RuleEngine {
    pub fn new() -> anyhow::Result<Self>;                 // Lua 5.4 sandbox: sem os, io, package, require, load
    pub fn load_default(&mut self) -> anyhow::Result<()>; // include_str!("../../../rules/default.lua")
    pub fn load_file(&mut self, path: &Path) -> anyhow::Result<()>;
    pub fn load_source(&mut self, name: &str, src: &str) -> anyhow::Result<()>;
    pub fn evaluate(&self, trace: &TraceResult, graph: &Graph) -> anyhow::Result<Vec<Finding>>;
    pub fn rule_count(&self) -> usize;
}
pub struct Config { pub theme: BTreeMap<String, String>, pub rules: Vec<PathBuf> }   // theme: nome → "#rrggbb"
pub fn load_config(path: Option<&Path>) -> anyhow::Result<Config>;   // default: $XDG_CONFIG_HOME/peabiru/config.lua; ausente → Config::default()
pub fn default_config_path() -> Option<PathBuf>;
```

API Lua disponível dentro de um arquivo de regras (cada arquivo é executado uma
vez por `evaluate`, dentro de um env próprio):

```lua
-- globais
finding(message [, severity])      -- severity: "info" | "warning" | "critical" (default "warning")
trace.confidence                   -- 0..100
trace.hops, trace.observed, trace.inferred, trace.correlations
flow.rtt_ms, flow.retransmissions, flow.drops, flow.rst   -- agregados do caminho (max rtt, somas)
path:contains(x)                   -- x = kind ("nat" | "load_balancer" | "internet_gateway" ...) ou nome de entidade
path.hops[i]                       -- { name, kind, status, address, confidence, rtt_ms, retransmissions, drops, rst }
source / destination               -- { name, ip, kind }, com :is_private()
```

`rules/default.lua` implementa pelo menos: retransmissões > 5 (warning),
rtt > 100 ms (warning), `drops > 0` (critical), tromboning (`source:is_private()
and path:contains("nat") and destination:is_private()` → warning "Possible
traffic tromboning"), confiança < 60 (info "Low path confidence").

`config.lua` de exemplo (documentar em README):

```lua
return {
  theme = { observed = "#5fd75f", correlated = "#e5a52e", title = "#5ec8c8" },
  rules = { "~/.config/peabiru/rules/*.lua" },
}
```

Precisões implementadas em `peabiru-lua` (aditivas ao contrato acima):

- **Sandbox** (`rules.rs`): o estado nasce com `Lua::new_with(STRING | TABLE |
  MATH | UTF8 | COROUTINE)` (`os`, `io`, `package` e `debug` nem são
  carregadas), e da biblioteca base ainda saem `require`, `load`, `loadfile`,
  `loadstring`, `dofile` e `collectgarbage`. Ficam `print`, `pairs`, `ipairs`,
  `tostring`, `tonumber`, `type`, `select`, `next`, `error`, `pcall`, `assert`,
  `rawget`/`rawset`, `setmetatable`, etc. `config.lua` roda no mesmo sandbox.
- **Ambiente por regra**: cada arquivo é compilado uma vez (`load_source`; erro
  de sintaxe → `Err`) e, a cada `evaluate`, recebe uma tabela `_ENV` nova
  (`__index` → globais do sandbox, `_G` → ela mesma) via
  `Function::set_environment`. Globais criadas por uma regra não vazam para
  outra nem para o próximo `evaluate`.
- **Erros de execução** viram `Finding { severity: Warning, rule: <nome>,
  message: "rule error: <primeira linha do erro>" }` (ex.: `rule error: runtime
  error: broken.lua:2: attempt to perform arithmetic on a nil value (local
  'x')`) e as demais regras rodam normalmente; `evaluate` só devolve `Err` para
  falhas do próprio runtime.
- **`Finding.rule`** = `"default.lua"` em `load_default`, nome do arquivo (sem
  diretório) em `load_file`, o `name` dado em `load_source`.
- **`finding(message [, severity [, subject]])`**: `message` aceita string ou
  número (ausente/tabela → rule error); `severity` é case-insensitive
  (`Severity::from_str`, aceita `warn`/`crit`), ausente ou inválida →
  `Warning`; `subject` (extra) é nome ou id de nó do trace (hops, branches,
  correlações) e vira `Finding.subject` com o id da entidade; texto
  desconhecido é guardado como veio.
- **Tabelas da API** (além de §5): `trace.id`, `trace.status` (`"completed"`,
  minúsculas), `trace.duration_ms`, `trace.request.{from,to,port,protocol}`;
  `flow.rtt_ms` é `max_rtt_ms()` ou **0** se nenhum hop tem RTT (idem
  `path.hops[i].rtt_ms`), para `flow.rtt_ms > 100` nunca quebrar;
  `path.hops[i]` traz também `index` (**1-based**, igual à posição no array),
  `entity`, `edge_status` e `edge_source` (`nil` no hop 1); `status`/`kind`
  são os rótulos em minúsculas (`"correlated"`, `"load_balancer"`);
  `path.branches[i] = { anchor (1-based), entity, name, kind, status, address,
  edge_status, direction ("outgoing" | "incoming") }`; `path:contains(x)`
  compara `x` (sem distinguir maiúsculas) com o kind **e** o nome de cada hop
  **e** de cada branch, e aceita `path.contains(x)`. No demo,
  `path:contains("nat")` é `true` por causa da branch `nat-gw`, então as
  regras padrão dão exatamente um finding no trace demo: `[warning] Possible
  traffic tromboning`. `source`/`destination` = `{ entity, name, kind, status,
  address, ip (string ou nil), port, hostname, provider }` do primeiro/último
  hop, com o endereço principal da entidade no grafo (entidade ausente →
  `Hop.address` é analisado) e `:is_private()` = `Address::is_private` (false
  sem IP; inclui CGNAT 100.64/10 e IPv6 mapeado de v4, ver §2.2).
- **`rules/default.lua`**: `"High TCP retransmission"` (> 5), `"High RTT on
  path (above 100 ms)"`, `"Kernel drops on path"` (critical), `"Possible
  traffic tromboning"`, `"Low path confidence"` (info, < 60); o cabeçalho do
  arquivo documenta a API para quem escreve regras.
- **Config** (`config.rs`): sem `dirs::config_dir()` → default; arquivo
  ausente → default mesmo com caminho explícito; o arquivo pode ser
  `return { ... }` ou só a expressão `{ ... }`; `theme` só aceita valores
  string (outro tipo → erro `theme.<chave>: expected a color string ...`) e
  chaves desconhecidas são mantidas; `rules` aceita lista ou uma única string;
  `~` / `~/...` → `dirs::home_dir()`; caminho relativo é resolvido a partir do
  diretório do `config.lua`; `*`/`?` só no nome do arquivo (no diretório →
  erro), expandidos com `read_dir` (só arquivos, ordem alfabética; diretório
  inexistente → nenhuma entrada); caminhos sem glob são mantidos mesmo que não
  existam (`RuleEngine::load_file` reporta depois).
- Extras públicos: `RuleEngine::rule_names() -> Vec<&str>`,
  `rules::{DEFAULT_RULES, DEFAULT_RULES_NAME}`, `parse_config(src, base_dir:
  Option<&Path>) -> anyhow::Result<Config>` (re-exportado em `lib.rs`),
  `Config: Clone + Default + PartialEq + Eq + Serialize + Deserialize`. `mlua`
  fica sem a feature `error-send`, então `mlua::Error` é convertido para
  `anyhow` pela mensagem (`rules::lua_err`).

## 6. peabiru-tui

### 6.1 Estado

```rust
pub struct App {
    pub tab: Tab,                        // Trace | Topology | Correlations | Observability | Settings
    pub focus: Panel,                    // TraceTarget | Graph | NodeDetails | Correlations | Events
    pub graph: Graph,
    pub request: TraceRequest,
    pub trace: Option<TraceResult>,
    pub run: RunState,                   // Idle | Running { started: Instant, next_event: usize } | Done
    pub events: Vec<Event>,              // eventos já emitidos (máx 500, FIFO)
    pub events_paused: bool,
    pub selected_node: usize,            // índice em `graph_nodes()` (hops + branches em ordem de desenho)
    pub selected_correlation: usize,
    pub selected_event: usize,
    pub yolo: bool,                      // auto-run contínuo: ao terminar, reinicia em 3 s
    pub help_open: bool,
    pub details_open: bool,
    pub environment: Environment,        // discovery::demo::Environment
    pub theme: Theme,
    pub findings: Vec<Finding>,
    pub tick: u64,                       // incrementa a cada 100 ms
    pub should_quit: bool,
}
impl App {
    pub fn new(graph: Graph, request: TraceRequest, environment: Environment, theme: Theme) -> Self;
    pub fn run_trace(&mut self);         // executa TraceEngine, zera eventos, entra em Running (sensor replay)
    pub fn on_tick(&mut self, sensor: &mut dyn Sensor);
    pub fn on_key(&mut self, key: KeyEvent);
    pub fn graph_nodes(&self) -> Vec<GraphNode>;      // ordem: hops[0..], depois branches (a mesma que o renderer usa)
    pub fn selected_entity(&self) -> Option<&Entity>;
}
pub struct GraphNode { pub entity: EntityId, pub name: String, pub status: NodeStatus, pub is_branch: bool }
```

### 6.2 Layout (mockups)

Terminal de referência: 156×50. Mínimo suportado: 100×30 (abaixo disso, tela
única "terminal too small: need 100x30").

```
┌─ linha 0..2 ───────────────────────────────────────────────────────────────────────┐
│ [logo 2 linhas: "PEABIRU" grande em amarelo + "NETWORK TRACING" em teal] │ tabs: [1] Trace [2] Topology [3] Correlations [4] Observability [5] Settings   ? Help  q Quit │ connected ● / peabiru.local:443 │
├─ área principal (Tab::Trace) ────────────────────────────────────────────────────────┤
│ esquerda 26%           │ centro 46%                      │ direita 28%                  │
│ TRACE TARGET (7 l.)    │ TRACE GRAPH                     │ NODE DETAILS (min 20 l.)     │
│ TRACE SUMMARY (10 l.)  │                                 │ CORRELATIONS (restante)      │
│ LEGEND (restante)      │                                 │                              │
├─ rodapé de painéis (8 linhas) ──────────────────────────────────────────────────────┤
│ EVENTS (66%)                                         │ KEYMAP (34%)                   │
├─ status (1 linha) ──────────────────────────────────────────────────────────────────┤
│ peabiru v0.1.0      environment: prod │ cluster: prod-e1 │ user: admin      press ? for help │
└────────────────────────────────────────────────────────────────────────────────────┘
```

Todos os painéis usam `Block` com `Borders::ALL`, `BorderType::Rounded`, título
em MAIÚSCULAS na cor `title`, borda `border` (ou `border_focus` quando o painel
tem foco). Fundo pintado com `theme.bg` (opção `--no-bg` desliga).

Painéis do Tab Trace:

- **TRACE TARGET**: linha 1 = alvo (`request.to`) em branco; `PORT:` porta +
  protocolo; `FROM:` origem; separador; `[R] Run Trace` (chave em amarelo).
  Enquanto `Running`, mostra spinner `⠋⠙⠹…` + "tracing…".
- **TRACE SUMMARY**: STATUS (Completed verde / Running amarelo / Idle cinza /
  Failed vermelho), DURATION `2.843s`, HOPS, OBSERVED `● 7 (50%)`, INFERRED
  `○ 7 (50%)`, CORRELATIONS, CONFIDENCE `78%` + barra de 12 blocos `■` (preenchidos
  verde, vazios `border`).
- **LEGEND**: `● observed node`, `○ inferred node`, `◇ correlated node`,
  `◆ probe point`, `── observed path`, `╌╌ inferred path`, `┄┄ possible path`,
  `→ direction`.
- **TRACE GRAPH**: ver 6.3.
- **NODE DETAILS**: cabeçalho `● aws-alb  [CORRELATED]` (glifo/cor pelo status,
  badge com borda na cor do status); pares ADDRESS/IP:PORT/TYPE/AS/REGION (o que
  existir em `addresses`/`attrs`; chaves de attrs em MAIÚSCULAS); separador; STATUS
  (`reachable` verde se attrs.reachable=="true", senão "unknown"), SOURCE
  `N sensors`, LAST SEEN (`x.xxxs ago` relativo ao tick), CONFIDENCE `92%` + barra;
  separador; `EVIDENCE (n)` lista `● <source>` (glifo pela kind: observacional `●`
  verde, senão `○`); rodapé `d view details   y yolo mode`.
- **CORRELATIONS**: título `CORRELATIONS (3)`; linhas `◇ a  <->  b  92%` (score
  verde ≥80, amarelo ≥60, vermelho abaixo); linha selecionada com fundo `selection`.
- **EVENTS**: título `EVENTS` + à direita `[p] pause` (ou `[p] resume` em amarelo).
  Colunas: hora `HH:MM:SS` (local), glifo+fonte (cor pelo `source.status()`),
  action, from, `→`/`↔`, to, detail alinhado à direita. Mostra as últimas N que
  cabem; auto-scroll salvo pausado.
- **KEYMAP**: `r run trace`, `tab next panel`, `↑/↓ navigate`, `enter select`,
  `d node details`, `y yolo mode`, `q quit` (tecla em amarelo, descrição em texto).
- **Rodapé**: esquerdo `peabiru v{CARGO_PKG_VERSION}`; centro `environment: prod
  │ cluster: prod-e1 │ user: admin` (rótulos `muted`, valores `title`); direito
  `press ? for help`.
- **Help (`?`)**: popup centralizado 60×20 com todas as teclas; qualquer tecla fecha.
- **Details (`d`/`enter` no grafo)**: popup 70×24 com todos os attrs, endereços,
  evidências (kind, source, provider, confidence, timestamp) e arestas
  (in/out com status e métricas) do nó selecionado; `Esc`/`d` fecha.

Outros tabs (versão simples, mas funcional):

- **[2] Topology**: tabela de todas as entidades (name, kind, provider, address,
  status, sources) + abaixo tabela de arestas (from → to, relation, status,
  confidence). `↑/↓` navega, `enter` seleciona como nó de detalhes.
- **[3] Correlations**: tabela completa (a, b, score, basis) + painel de detalhe
  da selecionada.
- **[4] Observability**: FINDINGS (severidade colorida, regra, mensagem) das regras
  Lua + métricas do caminho (rtt máx, retransmissões, drops, rst) + contadores de
  eventos por fonte.
- **[5] Settings**: tema atual (nome → cor, amostra colorida), caminho do
  config.lua, regras carregadas (arquivo → nº de findings), provider ativo,
  tamanho do terminal, flags (`yolo`, `paused`).

### 6.3 Renderer do grafo (`ui/graph.rs`)

Desenha num `Buffer` local (grade de células) e depois copia para a área do
painel, com scroll vertical quando não couber (`selected_node` sempre visível).

Regras de posicionamento:

- Coluna central `cx = área.width * 0.42` (o caminho principal fica um pouco à
  esquerda do centro, como no mockup, deixando espaço para rótulos à direita).
- Cada hop ocupa **4 linhas**: linha do nó (glifo em `cx`, nome em `cx+3`),
  linha do endereço (`cx+3`, cor `muted`), linha da seta (`│` em `cx`), linha da
  ponta (`▼` em `cx`). Último hop sem seta.
- Glifos por status: Observed `●`, Inferred `○`, Correlated `◇`, Probe `◆`.
  Nó selecionado: glifo com `Modifier::REVERSED` + nome em **bold**.
- Segmento vertical entre hops: `│` verde se `edge_status.is_observed()`, `┆`
  em `muted` se inferido. Ponta `▼` na mesma cor.
- Branches: alternam lado esquerdo/direito por hop âncora (primeira branch de um
  hop vai para a esquerda se `cx > width/2`, senão direita; as demais alternam).
  Desenhadas na linha do nó âncora + 2 (mesma linha da seta), como um "L"
  tracejado: horizontal `╌` a partir de `cx±1` até a coluna da branch, depois
  vertical `╎` 1–2 linhas até a linha do glifo da branch. Glifo/nome/endereço da
  branch como um hop, mas nome em `muted` quando Inferred. Branch Correlated usa
  `╌` em `correlated` (laranja), como `orders-db` no mockup. Lado esquerdo: glifo
  em `cx - 24` (mínimo 2), nome à direita do glifo; lado direito: glifo em
  `cx + 26` (máximo width-20).
- Não sobrepor: se duas branches caírem na mesma linha/lado, empilhar com +3
  linhas. Se não houver largura para o lado, cair para o outro.
- Título do painel `TRACE GRAPH`; quando `Running`, sufixo ` · tracing…` com
  spinner; quando nenhum trace, texto central `press r to run a trace`.

### 6.4 Teclas (`input.rs`)

| tecla | ação |
|---|---|
| `1`–`5` | troca de tab |
| `?` | abre/fecha help |
| `q` / `Ctrl-C` | sair (com popup aberto, só fecha o popup) |
| `r` | `run_trace()` |
| `Tab` / `Shift-Tab` | próximo/anterior painel focado (TraceTarget → Graph → NodeDetails → Correlations → Events) |
| `↑`/`↓`, `k`/`j` | navega no painel focado (Graph: `selected_node`; Correlations; Events) |
| `Enter` | Graph: abre details; Correlations: seleciona nó `a` da correlação e foca Graph |
| `d` | abre/fecha details do nó selecionado |
| `y` | alterna yolo mode (indicador `YOLO` amarelo no header, ao lado de connected) |
| `p` | pausa/retoma eventos |
| `Esc` | fecha popup; senão volta ao Tab Trace |
| `g`/`G` | primeiro/último item do painel focado |

### 6.5 Tema (`theme.rs`)

```rust
#[derive(Clone, Debug)]
pub struct Theme {
    pub bg: Color, pub panel_bg: Color, pub border: Color, pub border_focus: Color,
    pub title: Color, pub text: Color, pub muted: Color, pub key: Color, pub selection: Color,
    pub logo: Color, pub logo_sub: Color,
    pub observed: Color, pub inferred: Color, pub correlated: Color, pub probe: Color,
    pub ok: Color, pub warn: Color, pub error: Color,
    pub paint_bg: bool,
}
impl Theme {
    pub fn default_dark() -> Self;
    pub fn apply_overrides(&mut self, overrides: &BTreeMap<String, String>) -> Vec<String>; // retorna chaves desconhecidas
    pub fn status_color(&self, s: NodeStatus) -> Color;
    pub fn names() -> &'static [&'static str];   // nomes aceitos no config.lua
}
```

Cores padrão (mockup 1): `bg #0b0e14`, `panel_bg #0b0e14`, `border #2b3240`,
`border_focus #e5b42e`, `title #5ec8c8`, `text #c8d0da`, `muted #7c8594`,
`key #e5b42e`, `selection #1c2330`, `logo #f2b134`, `logo_sub #5ec8c8`,
`observed #5fd75f`, `inferred #c8d0da`, `correlated #e5a52e`, `probe #a78bfa`,
`ok #5fd75f`, `warn #e5b42e`, `error #f06c6c`.

### 6.6 Runtime e snapshot

```rust
pub fn run(app: App, sensor: Box<dyn Sensor>) -> anyhow::Result<()>;   // raw mode + alternate screen; restaura no drop e em panic
pub fn render_to_string(app: &App, width: u16, height: u16, ansi: bool) -> String; // TestBackend; ansi=true emite SGR truecolor por célula
```

`render_to_string` é a base dos testes de snapshot (`crates/peabiru-tui/tests/snapshot.rs`
compara com `tests/snapshots/trace_156x50.txt` e `trace_100x30.txt`; o teste
falha com diff legível se divergir).

```rust
pub mod snapshot {
    pub const FIXED_BASE_UNIX: i64;              // 2026-01-01T12:41:02Z
    pub fn fixed_base() -> Timestamp;            // base dos eventos/evidências do demo nos renders determinísticos
    pub fn fixed_clock() -> Timestamp;           // fixed_base() + DEMO_DURATION_MS: o "agora" do LAST SEEN
    pub fn prepare(app: &mut App, sensor: &mut dyn Sensor, clock: Timestamp); // estado final de --render-once
}
```

`prepare` fixa `app.fixed_clock = Some(clock)`, chama `run_trace()`, esgota
o sensor via `on_tick` (até um tick sem eventos novos; máx. 100), encerra o
run (`Done`, STATUS `Completed`) e seleciona o primeiro hop `Correlated` (o
`aws-alb` do mockup). `peabiru tui --render-once` e `tests/snapshot.rs`
constroem o **mesmo** estado (`App::new(demo::graph_from(fixed_base()),
..)` + `DemoSensor::with_base(0, fixed_base())` + `prepare(.., fixed_clock())`),
e por isso o render é determinístico: horas em UTC (`12:41:02`…), `LAST
SEEN 1.700s ago`. O teste não avalia regras Lua (o Tab Trace não desenha
findings e `peabiru-tui` não depende de `peabiru-lua`);
`PEABIRU_UPDATE_SNAPSHOTS=1` regrava os arquivos em vez de comparar.

Precisões implementadas em `peabiru-tui` (aditivas ao contrato acima):

- **Campos extras de `App`**: `pub last_error: Option<String>` (erro do último
  `run_trace()`; STATUS `Failed` no TRACE SUMMARY via `App::trace_status() ->
  TraceStatus`) e privados `timeline`, `replay_len`, `pending`, `done_tick`
  (abaixo). `App: Clone + Debug`. Constantes públicas `MAX_EVENTS = 500`,
  `TICK_MS = 100`, `YOLO_RERUN_TICKS = 30`, `RUN_MIN_MS = 1000`.
- **`App.findings_fn: Option<FindingsFn>`** (`app::FindingsFn(pub
  Rc<FindingsEval>)`, `FindingsEval = dyn Fn(&TraceResult, &Graph) ->
  Vec<Finding>`; `Clone` + `Debug` manuais para `App` continuar `Clone +
  Debug`): avaliador chamado ao fim de `run_trace()`.
  `findings` é zerado e, havendo trace e avaliador, recebe o resultado
  (copiado também em `trace.findings`). É por aqui que o CLI liga o
  `RuleEngine` da `peabiru-lua` sem a TUI depender dela; `None` (padrão)
  deixa `findings` vazio. O rerun do yolo passa pelo mesmo caminho.
- **`App.fixed_clock: Option<Timestamp>`** e `App::now() -> Timestamp`
  (`fixed_clock` ou `Utc::now()`): o "agora" do `LAST SEEN` e dos eventos
  re-emitidos no replay; com `fixed_clock` definido, EVENTS e o popup de
  details formatam `HH:MM:SS` em UTC em vez de hora local (`widgets::
  fmt_clock(ts, utc)`). `None` por padrão; `snapshot::prepare` define.
- **Replay via `Sensor`** (`on_tick`): a cada tick o sensor recebe `poll`;
  cada evento novo entra numa `timeline` interna (máx. 500, FIFO) **e** é
  exibido ao vivo. `run_trace()` zera `events` e entra em `Running { started,
  next_event: 0 }`; enquanto `Running`, os eventos da timeline anteriores ao
  run são re-emitidos (com `ts = agora`) quando `elapsed >= offset` (offset =
  `ts − ts do primeiro evento da timeline`), `next_event` é o cursor desse
  replay, e o run termina (`Done`) quando `elapsed >= max(summary.duration_ms,
  RUN_MIN_MS)`. `TraceError` → `trace = None`, `last_error = Some(..)`, `Done`
  imediato. Assim o rerun (yolo) reproduz a timeline do demo nos offsets
  originais sem que o trait `Sensor` precise de `reset`.
- **Yolo**: contado em ticks a partir do `Done` (`YOLO_RERUN_TICKS` = 3 s);
  ligar yolo em `Idle` dispara o primeiro trace na hora.
- **Pausa (`p`)**: enquanto pausado, eventos novos ficam numa fila `pending`
  (máx. 500) e entram todos de uma vez no resume; `selected_event` acompanha o
  último evento só quando não pausado (auto-scroll).
- **Seleção**: `selection_len()` e `select_next/prev/first/last` (wrap-around)
  operam no painel focado do Tab Trace (Graph → `selected_node`, Correlations
  → `selected_correlation`, Events → `selected_event`; TraceTarget/NodeDetails
  não navegam) e, nos outros tabs, Topology → `selected_node` como índice em
  `graph.entities()` (`selected_entity()` devolve essa entidade), Correlations
  → `selected_correlation`. `Enter` (`activate_selection`): Graph/Topology
  abre details; Correlations (painel ou tab) seleciona o nó `a`
  (`select_correlation_node`), foca o Graph e volta ao Tab Trace. `set_tab`
  recorta as seleções para o novo contexto. Foco inicial: `Panel::Graph`.
- **Popups**: com help aberto, qualquer tecla fecha; com details aberto só
  `Esc`/`d`/`q`/`Enter`/`Ctrl-C` fecham e as demais são ignoradas.
- **Layout**: `ui::layout(area) -> Areas` e `ui::trace_layout(main) ->
  TraceAreas` são públicos (usados pelos testes); EVENTS/KEYMAP e a linha de
  status aparecem em todos os tabs; só a área principal muda. Coluna
  esquerda = `[Length(TRACE_TARGET_HEIGHT = 7), Length(TRACE_SUMMARY_HEIGHT =
  10), Fill(1)]`. TRACE TARGET tem 7 linhas (5 de conteúdo: alvo, PORT,
  FROM, separador, ação) e, se a LEGEND ficar com menos de
  `LEGEND_MIN_HEIGHT` (3) linhas, elas passam ao TRACE SUMMARY e o painel não
  é desenhado (é o caso a 100×30). Coluna direita = `[Min(20), Fill(1)]` (a
  100×30 CORRELATIONS fica sem altura; o tab `[3]` mostra a tabela). Tela
  pequena: linha centralizada `terminal too small: need 100x30 (have WxH)`.
- **Widgets**: títulos de painel saem como `" TÍTULO "` (um espaço de cada
  lado, colado à borda). `panel_block` põe o título em maiúsculas;
  `panel_block_line(theme, Line, focused)` recebe o título pronto (sufixo
  ` · tracing… ⠋` do TRACE GRAPH, nome do nó no popup de details). Extras:
  `kv_line_width` (`KEY_WIDTH = 13`), `evidence_glyph`, `fill_bg_buf`,
  `SPINNER`, `Theme::{score_color, status, selection, get}` e
  `theme::parse_hex_color` (`#rrggbb` ou `#rgb`, `#` opcional);
  `apply_overrides` devolve, além das chaves desconhecidas, `"<chave>: invalid
  color '<valor>'"` para valores inválidos (a cor não muda).
- **`render_to_string`**: cada linha termina em `\n` (inclusive a última);
  modo ANSI emite SGR (`ESC[0;<mods>;<fg>;<bg>m`) só quando o estilo muda e
  `ESC[0m` no fim de cada linha. `snapshot::buffer_to_string(&Buffer, ansi)`
  é público. Erros do `TestBackend` viram a string `render error: ...`.
- **`run`**: `ratatui::try_init()` (erro via `anyhow` em vez de panic) e
  `restore()` num guard com `Drop`, além do panic hook do ratatui; só
  `KeyEventKind::Press` chega ao `App`.
- Helpers públicos: `Tab::{ALL, number, label, from_digit}`,
  `Panel::{ORDER, next, prev, title}`, `RunState::is_running`,
  `App::{next_panel, prev_panel, toggle_help, toggle_details, toggle_yolo,
  toggle_pause, set_tab, run_elapsed_ms}`.
- **Header / rodapé / EVENTS / KEYMAP** (`ui/{header,footer,events,keymap}.rs`):
  o header são três blocos arredondados (logo | tabs | conexão) com uma linha
  interna; logo e conexão têm a largura do próprio conteúdo e as tabs ocupam o
  resto. Quando não cabe, degrada em passos fixos em vez de recortar: separador
  simples entre tabs → `? Help q Quit` compacto → sem endpoint → sem dicas →
  sem `NETWORK TRACING` → rótulos `Correl.`/`Observ.` (em 156 colunas só o
  `YOLO` força o primeiro passo; em 100 caem endpoint, subtítulo e dicas).
  Rodapé: separador `  │  ` vira ` │ ` se não couber. EVENTS: texto com margem
  de 1 coluna; colunas fixas hora (8) + glifo (2) + fonte (12) + action (8);
  `from` preenchido até a maior largura entre todos os eventos (reduzido a pelo
  menos metade do restante quando não cabe), `to` solto (preenchido só se
  sobrar espaço) e `detail` alinhado à direita, oculto abaixo de 90 colunas de
  texto; recortes usam `…`; a linha selecionada leva fundo `selection` quando
  pausado ou com foco; vazio → `waiting for events… press r to run a trace`.
  KEYMAP: tecla preenchida até 6 colunas; inclui `p pause events`; se as 8
  entradas não cabem na altura mas cabem na largura, duas colunas balanceadas
  (4 + 4), senão as primeiras que cabem. Helpers públicos:
  `ui::events::{fit_to, pad_to}`, `ui::keymap::{BINDINGS, KEY_COL}`.

- **NODE DETAILS / CORRELATIONS**: no NODE DETAILS, quando `EVIDENCE (n)`
  + todas as evidências não cabem, a linha em branco abaixo do cabeçalho cai
  antes de a lista encolher para `… +N more` (a 100×30 as duas evidências do
  `aws-alb` aparecem). No CORRELATIONS os nomes saem inteiros sempre que
  cabem, preenchidos até as larguras nominais 14/12 quando sobra espaço, com
  o separador `  <->  ` ou, se só assim couberem, o compacto ` <-> ` (é o
  caso das 40 colunas de conteúdo a 156×50: `◇ istio-ingressgateway <-> redis
  73%`); só depois valem as larguras nominais e, em painéis estreitos, a
  redução proporcional com `…` (mín. 4 por coluna). O score fica alinhado à
  direita em todos os casos.
- **TRACE GRAPH** (`ui/graph.rs`): entidade com vários endereços mostra
  todos em linhas consecutivas (hop de `4 + n−1` linhas, como `aws-alb`). O
  "L" de uma branch sai da **linha do nó** âncora (à esquerda `╭╌╌` de
  `cx−1` até a coluna do glifo, à direita `╌╌╮` logo depois do nome) e desce
  um `╎` até o glifo, na linha do âncora + 2. Primeiro lado pela regra de §6.3
  (com `cx = 42%`, direita); sem espaço (nome/endereços do hop alcançam a
  coluna do canto, ou menos de 4 colunas para texto) cai para o outro; no
  último hop uma branch pode descer em linha reta de `cx` (`╎` e glifo em
  `y0 + max(3, endereços + 1)`); as demais empilham `+3` linhas partilhando o
  `╎` do lado e, por fim, deslocam a coluna 3/6/9 para dentro. Uma grade de
  ocupação impede sobreposição; nome/endereço que não cabem são cortados com
  `…`; conteúdo maior que o painel rola centrado no nó selecionado, com
  `↑ more`/`↓ more` em `muted` no canto direito. `graph::layout_height(app,
  width)` é público (altura do conteúdo, 0 sem trace). No demo a 156×50:
  `nat-gw` à esquerda do istio, `redis` à direita, `orders-db` à esquerda e
  `payments-svc` em linha reta abaixo de `app-svc`; a 100×30 o grafo não cabe
  (27 linhas em 16) e rola.

## 7. peabiru-cli

```
peabiru                                  # = peabiru tui
peabiru tui [--provider demo] [--config FILE] [--rules FILE]... [--no-bg]
            [--render-once --size WxH [--ansi]]
peabiru trace --from <selector> --to <selector|fqdn> [--port 443] [--protocol HTTPS] [--json] [--rules FILE]...
peabiru graph [--json]                   # dump do grafo global (tabela ou JSON)
peabiru rules check <FILE>...            # carrega e executa contra o trace demo, imprime findings
peabiru --version
```

Saída JSON de `trace` = `serde_json::to_string_pretty(&TraceResult)`.
Códigos de saída: 0 ok; 2 erro de argumento; 3 trace falhou (sem caminho).

Precisões implementadas em `peabiru-cli` (aditivas ao contrato acima):

- **Módulos**: `args.rs` (clap derive: `Cli { cmd: Option<Cmd>, --config,
  --rules, --no-bg }`, `Cmd::{Tui(TuiArgs), Trace(TraceArgs), Graph{--json},
  Rules{Check{files}}}`), `bootstrap.rs`, `commands.rs`, `table.rs`. Sem
  subcomando = `tui` com os padrões (`TuiArgs::default()`: provider `demo`,
  `156x50`). `--config`, `--rules` e `--no-bg` são globais: valem antes ou
  depois do subcomando; `--rules` repetido no **mesmo** nível acumula, mas
  uma ocorrência depois do subcomando substitui a de antes (semântica do
  clap, que não mescla níveis).
- **Bootstrap**: `load_config` (`peabiru_lua::load_config(--config)`; config
  inválido é erro (exit 1) em `tui`, `trace` e `graph`; `rules check` não
  lê config), `build_graph(provider)` (`registry()`, `discover` em grafo novo,
  na `tui`, só o provider de `--provider`; avisos e falhas de provider vão
  para stderr como `warning: provider <nome>: ...` sem interromper),
  `build_engine` (`load_default`, depois `config.rules`, cuja falha vira aviso,
  e `--rules`, cuja falha é erro), `build_theme` (`Theme::default_dark()` +
  `apply_overrides(config.theme)`; rejeitados viram `warning: config.lua
  theme: unknown key '<k>' (known: ...)` / `warning: config.lua theme: <k>:
  invalid color '<v>'`; `paint_bg = !--no-bg`).
- **`tui`**: `--provider` fora do registro → erro de argumento do clap (exit
  2) listando os disponíveis. `App::new(graph, demo::default_request(),
  demo::environment(), theme)` com `findings_fn` ligado ao `RuleEngine`
  (falha do runtime vira um finding `warning` de regra `rules`: `rule engine
  error: ...`). Interativo: `peabiru_tui::run(app, DemoSensor::new(350 ms))`,
  sem trace automático (o usuário aperta `r`). `--render-once` é o frame de
  referência dos snapshots e por isso é determinístico: o grafo é
  `demo::graph_from(snapshot::fixed_base())` (ignora `--provider`, que só é
  validado), `snapshot::prepare(&mut app, &mut DemoSensor::with_base(0,
  fixed_base()), snapshot::fixed_clock())` faz `run_trace()`, esgota o
  sensor (todos os eventos entram no EVENTS), encerra o run
  (`RunState::Done`, STATUS `Completed`) e seleciona o primeiro hop
  `Correlated` (o `aws-alb` do mockup) para o NODE DETAILS; depois imprime
  `render_to_string(&app, W, H, --ansi)`. `--size` aceita `WxH` ou `WXH`,
  zero é erro de argumento.
- **`trace`**: `TraceRequest::new(from, to, port, protocol)`; `TraceError` →
  `error: <msg>` em stderr e exit 3 (stdout vazio); `result.findings` =
  `engine.evaluate(...)`; `--json` = `to_string_pretty`, senão
  `render_ascii()`.
- **`graph`**: tabela `NAME KIND PROVIDER ADDRESS STATUS` (endereço principal
  via `Address::display`, `Entity::status().label()`), linha em branco, tabela
  `FROM TO RELATION STATUS CONF` (nomes das entidades, ou o id se ausente,
  `Relation::label()`, `Edge::status()`, `Edge::confidence()` como `NN%`);
  colunas alinhadas à esquerda, separadas por dois espaços, sem espaços
  finais. `--json` = `to_string_pretty(&Graph)`.
- **`rules check`**: engine novo com `load_default` e depois os arquivos na
  ordem dada (falha de carga → exit 1; o mesmo arquivo passado duas vezes
  roda duas vezes, exceto um arquivo cujo conteúdo é igual ao
  `rules/default.lua` embutido, que é pulado com `note: ... (already
  loaded)` em stderr para não duplicar cada finding); avalia sobre
  `demo::graph()` + `demo::default_request()`;
  stdout `[SEVERITY] rule: message` (severidade em maiúsculas), resumo
  (`loaded N rule file(s): ...; M finding(s) on the demo trace`) em stderr.
- **Códigos de saída**: 0 ok; 1 erro de execução (`error: <anyhow>` em
  stderr); 2 argumento (clap); 3 `TraceError`.
- **Testes** (`tests/cli.rs`): rodam o binário (`CARGO_BIN_EXE_peabiru`) com
  `--config` apontando para um arquivo inexistente, isolando do `config.lua`
  do usuário.

## 8. Convenções

- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` limpos.
- Sem `unwrap()` fora de testes; erros via `anyhow` no binário e `thiserror` nas libs.
- Nomes de módulos/símbolos em inglês; comentários e docs podem ser em português.
- Nada de `unsafe`.
- Mensagens de commit: conventional commits, curtas, sem trailers de IA.
