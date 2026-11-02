//! Regras de diagnóstico em Lua (VISION §22–23). O `RuleEngine` carrega
//! arquivos de regras num sandbox Lua 5.4 (sem `os`, `io`, `package`,
//! `require`, `load`, ...) e os executa contra um `TraceResult`, coletando
//! `Finding`s.
//!
//! Cada arquivo é executado **uma vez por `evaluate`**, num ambiente (`_ENV`)
//! próprio que expõe `finding`, `trace`, `flow`, `path`, `source` e
//! `destination` — a API completa está documentada no cabeçalho de
//! `rules/default.lua`. Um erro em tempo de execução numa regra vira um
//! `Finding` de aviso (`"rule error: ..."`) e não impede as demais.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::rc::Rc;

use anyhow::{anyhow, Context};
use mlua::{Function, Lua, LuaOptions, StdLib, Table, Value};
use peabiru_core::{Address, Finding, Graph, Severity};
use peabiru_trace::{Branch, Direction, Hop, TraceResult};

/// Fonte de `rules/default.lua`, embutida no binário.
pub const DEFAULT_RULES: &str = include_str!("../../../rules/default.lua");

/// Nome usado em `Finding.rule` pelas regras padrão.
pub const DEFAULT_RULES_NAME: &str = "default.lua";

/// Globais removidas do sandbox. `os`, `io`, `package` e `debug` nem chegam a
/// ser carregadas (ver `new_sandboxed_lua`); as demais vêm da biblioteca base.
const BLOCKED_GLOBALS: &[&str] = &[
    "os",
    "io",
    "package",
    "require",
    "load",
    "loadfile",
    "loadstring",
    "dofile",
    "debug",
    "collectgarbage",
];

/// `mlua::Error` não é `Send + Sync` sem a feature `error-send`, então não
/// converte com `?` para `anyhow`; este é o adaptador usado no crate.
pub(crate) fn lua_err(err: mlua::Error) -> anyhow::Error {
    anyhow!("{err}")
}

/// Estado Lua 5.4 com o sandbox do Peabiru: bibliotecas `string`, `table`,
/// `math`, `utf8`, `coroutine` e a base sem `load`/`loadfile`/`dofile`/
/// `collectgarbage`. Sem `os`, `io`, `package`, `require`, `debug`.
pub(crate) fn new_sandboxed_lua() -> anyhow::Result<Lua> {
    let libs = StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE;
    let lua = Lua::new_with(libs, LuaOptions::default()).map_err(lua_err)?;
    let globals = lua.globals();
    for name in BLOCKED_GLOBALS {
        globals
            .raw_set(*name, Value::Nil)
            .map_err(lua_err)
            .with_context(|| format!("sandbox: cannot remove global `{name}`"))?;
    }
    Ok(lua)
}

/// Uma regra carregada: nome (vai em `Finding.rule`) + chunk compilado.
struct Rule {
    name: String,
    chunk: Function,
}

/// Motor de regras: um estado Lua sandboxed e as regras carregadas, na ordem
/// de carga.
pub struct RuleEngine {
    // `rules` antes de `lua` para as referências caírem antes do estado.
    rules: Vec<Rule>,
    lua: Lua,
}

impl RuleEngine {
    /// Cria o engine com o sandbox e nenhuma regra.
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            rules: Vec::new(),
            lua: new_sandboxed_lua()?,
        })
    }

    /// Carrega `rules/default.lua` (embutido) com o nome `"default.lua"`.
    pub fn load_default(&mut self) -> anyhow::Result<()> {
        self.load_source(DEFAULT_RULES_NAME, DEFAULT_RULES)
    }

    /// Carrega um arquivo de regras; `Finding.rule` recebe o nome do arquivo
    /// (sem diretório).
    pub fn load_file(&mut self, path: &Path) -> anyhow::Result<()> {
        let src = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read rules file {}", path.display()))?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map_or_else(|| path.display().to_string(), str::to_string);
        self.load_source(&name, &src)
    }

    /// Compila `src` como um chunk chamado `name`. Erros de sintaxe são
    /// devolvidos aqui; erros de execução aparecem como findings em `evaluate`.
    pub fn load_source(&mut self, name: &str, src: &str) -> anyhow::Result<()> {
        let chunk = self
            .lua
            .load(src)
            .set_name(format!("={name}"))
            .into_function()
            .map_err(lua_err)
            .with_context(|| format!("cannot compile rules {name}"))?;
        self.rules.push(Rule {
            name: name.to_string(),
            chunk,
        });
        Ok(())
    }

    /// Quantidade de regras (arquivos/chunks) carregadas.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Nomes das regras carregadas, na ordem de carga.
    pub fn rule_names(&self) -> Vec<&str> {
        self.rules.iter().map(|r| r.name.as_str()).collect()
    }

    /// Executa todas as regras contra `trace`, cada uma em um ambiente novo.
    /// Uma regra que falha em tempo de execução contribui com um `Finding`
    /// `Warning` (`"rule error: ..."`) e as demais continuam. `Err` só para
    /// falhas do próprio runtime.
    pub fn evaluate(&self, trace: &TraceResult, graph: &Graph) -> anyhow::Result<Vec<Finding>> {
        let sink: Rc<RefCell<Vec<Finding>>> = Rc::new(RefCell::new(Vec::new()));
        let subjects = Rc::new(subject_index(trace));
        for rule in &self.rules {
            let env = self
                .environment(rule, trace, graph, &sink, &subjects)
                .map_err(lua_err)
                .with_context(|| format!("cannot build environment for rules {}", rule.name))?;
            let bound = rule.chunk.set_environment(env).map_err(lua_err)?;
            if !bound {
                return Err(anyhow!("cannot bind environment for rules {}", rule.name));
            }
            if let Err(err) = rule.chunk.call::<()>(()) {
                let message = format!("rule error: {}", first_line(&err.to_string()));
                sink.borrow_mut()
                    .push(Finding::new(Severity::Warning, &rule.name, &message));
            }
            // Solta o ambiente (e os callbacks Rust que ele segura).
            let _ = rule.chunk.set_environment(self.lua.globals());
        }
        let findings = std::mem::take(&mut *sink.borrow_mut());
        Ok(findings)
    }

    /// Ambiente de uma regra: tabela nova com `__index` → globais do sandbox,
    /// `_G` apontando para ela mesma, e as tabelas da API.
    fn environment(
        &self,
        rule: &Rule,
        trace: &TraceResult,
        graph: &Graph,
        sink: &Rc<RefCell<Vec<Finding>>>,
        subjects: &Rc<BTreeMap<String, String>>,
    ) -> mlua::Result<Table> {
        let lua = &self.lua;
        let env = lua.create_table()?;
        let meta = lua.create_table()?;
        meta.raw_set("__index", lua.globals())?;
        env.set_metatable(Some(meta));
        env.raw_set("_G", env.clone())?;
        env.raw_set(
            "finding",
            finding_function(lua, &rule.name, sink, subjects)?,
        )?;
        env.raw_set("trace", trace_table(lua, trace)?)?;
        env.raw_set("flow", flow_table(lua, trace)?)?;
        env.raw_set("path", path_table(lua, trace)?)?;
        env.raw_set("source", endpoint_table(lua, trace.hops.first(), graph)?)?;
        env.raw_set(
            "destination",
            endpoint_table(lua, trace.hops.last(), graph)?,
        )?;
        Ok(env)
    }
}

/// Primeira linha de uma mensagem de erro (tracebacks ficam de fora).
fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("").trim()
}

/// Nome ou id de nó do trace (hops, branches, correlações) → id da entidade,
/// para o argumento `subject` de `finding`.
fn subject_index(trace: &TraceResult) -> BTreeMap<String, String> {
    let mut index: BTreeMap<String, String> = BTreeMap::new();
    let mut add = |name: &str, id: &str| {
        index
            .entry(name.to_string())
            .or_insert_with(|| id.to_string());
        index
            .entry(id.to_string())
            .or_insert_with(|| id.to_string());
    };
    for h in &trace.hops {
        add(&h.name, &h.entity);
    }
    for b in &trace.branches {
        add(&b.name, &b.entity);
    }
    for c in &trace.correlations {
        add(&c.a_name, &c.a);
        add(&c.b_name, &c.b);
    }
    index
}

/// String ou número → `Some(texto)`; outros valores → `None`.
fn value_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => s.to_str().ok().map(|s| String::from(&*s)),
        Value::Integer(i) => Some(i.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// `"info"` | `"warning"` | `"critical"` (sem distinguir maiúsculas; `warn`/
/// `crit` também valem). Ausente ou inválida → `Warning`.
fn parse_severity(v: &Value) -> Severity {
    value_to_string(v)
        .and_then(|s| s.parse().ok())
        .unwrap_or(Severity::Warning)
}

/// `"OBSERVED"` → `"observed"` etc.
fn lower(label: &str) -> String {
    label.to_ascii_lowercase()
}

/// `finding(message [, severity [, subject]])`.
fn finding_function(
    lua: &Lua,
    rule: &str,
    sink: &Rc<RefCell<Vec<Finding>>>,
    subjects: &Rc<BTreeMap<String, String>>,
) -> mlua::Result<Function> {
    let rule = rule.to_string();
    let sink = Rc::clone(sink);
    let subjects = Rc::clone(subjects);
    lua.create_function(
        move |_, (message, severity, subject): (String, Value, Value)| {
            let mut finding = Finding::new(parse_severity(&severity), &rule, &message);
            if let Some(s) = value_to_string(&subject) {
                finding.subject = Some(subjects.get(&s).cloned().unwrap_or(s));
            }
            sink.borrow_mut().push(finding);
            Ok(())
        },
    )
}

/// `trace`: resumo do trace + `request`.
fn trace_table(lua: &Lua, trace: &TraceResult) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("id", trace.id)?;
    t.raw_set("status", lower(trace.status.label()))?;
    let s = &trace.summary;
    t.raw_set("confidence", s.confidence.pct())?;
    t.raw_set("hops", s.hops)?;
    t.raw_set("observed", s.observed)?;
    t.raw_set("inferred", s.inferred)?;
    t.raw_set("correlations", s.correlations)?;
    t.raw_set("duration_ms", s.duration_ms)?;
    let req = lua.create_table()?;
    req.raw_set("from", trace.request.from.as_str())?;
    req.raw_set("to", trace.request.to.as_str())?;
    req.raw_set("port", trace.request.port)?;
    req.raw_set("protocol", trace.request.protocol.as_str())?;
    t.raw_set("request", req)?;
    Ok(t)
}

/// `flow`: agregados do caminho (RTT máximo — 0 se desconhecido — e somas).
fn flow_table(lua: &Lua, trace: &TraceResult) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("rtt_ms", trace.max_rtt_ms().unwrap_or(0.0))?;
    t.raw_set("retransmissions", trace.total_retransmissions())?;
    t.raw_set("drops", trace.total_drops())?;
    t.raw_set("rst", trace.total_rst())?;
    Ok(t)
}

/// `path.hops[i]` (índice 1-based, igual à posição no array).
fn hop_table(lua: &Lua, hop: &Hop, index: usize) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("index", index)?;
    t.raw_set("entity", hop.entity.as_str())?;
    t.raw_set("name", hop.name.as_str())?;
    t.raw_set("kind", hop.kind.label())?;
    t.raw_set("status", lower(hop.status.label()))?;
    t.raw_set("address", hop.address.as_str())?;
    t.raw_set("confidence", hop.confidence.pct())?;
    t.raw_set("edge_status", lower(hop.edge_status.label()))?;
    t.raw_set("edge_source", hop.edge_source.clone())?;
    t.raw_set("rtt_ms", hop.metrics.rtt_ms.unwrap_or(0.0))?;
    t.raw_set("retransmissions", hop.metrics.retransmissions)?;
    t.raw_set("drops", hop.metrics.drops)?;
    t.raw_set("rst", hop.metrics.rst)?;
    Ok(t)
}

/// `path.branches[i]` (`anchor` 1-based, como `path.hops`).
fn branch_table(lua: &Lua, branch: &Branch) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("anchor", branch.anchor + 1)?;
    t.raw_set("entity", branch.entity.as_str())?;
    t.raw_set("name", branch.name.as_str())?;
    t.raw_set("kind", branch.kind.label())?;
    t.raw_set("status", lower(branch.status.label()))?;
    t.raw_set("address", branch.address.as_str())?;
    t.raw_set("edge_status", lower(branch.edge_status.label()))?;
    let direction = match branch.direction {
        Direction::Outgoing => "outgoing",
        Direction::Incoming => "incoming",
    };
    t.raw_set("direction", direction)?;
    Ok(t)
}

/// `path`: `hops`, `branches` e `contains(x)` — `x` é um kind
/// (`"nat"`, `"load_balancer"`, ...) ou nome de nó; procura em hops **e**
/// branches sem distinguir maiúsculas. Aceita `path:contains(x)` e
/// `path.contains(x)`.
fn path_table(lua: &Lua, trace: &TraceResult) -> mlua::Result<Table> {
    let path = lua.create_table()?;
    let hops = lua.create_table()?;
    for (i, hop) in trace.hops.iter().enumerate() {
        hops.raw_seti(i + 1, hop_table(lua, hop, i + 1)?)?;
    }
    path.raw_set("hops", hops)?;
    let branches = lua.create_table()?;
    for (i, branch) in trace.branches.iter().enumerate() {
        branches.raw_seti(i + 1, branch_table(lua, branch)?)?;
    }
    path.raw_set("branches", branches)?;

    let needles: Vec<(String, String)> = trace
        .hops
        .iter()
        .map(|h| (h.kind.label().to_string(), h.name.to_lowercase()))
        .chain(
            trace
                .branches
                .iter()
                .map(|b| (b.kind.label().to_string(), b.name.to_lowercase())),
        )
        .collect();
    let contains = lua.create_function(move |_, (first, second): (Value, Value)| {
        // `path:contains(x)` passa (path, x); `path.contains(x)` passa só (x).
        let needle = if second.is_nil() { &first } else { &second };
        let Some(needle) = value_to_string(needle) else {
            return Ok(false);
        };
        let needle = needle.trim().to_lowercase();
        Ok(needles
            .iter()
            .any(|(kind, name)| *kind == needle || *name == needle))
    })?;
    path.raw_set("contains", contains)?;
    Ok(path)
}

/// `"10.0.2.34:443"` | `"10.0.1.5"` | `"[::1]:443"` | `"host:443"` | `"host"`.
fn parse_address(s: &str) -> Option<Address> {
    if s.is_empty() {
        return None;
    }
    if let Ok(sa) = s.parse::<SocketAddr>() {
        return Some(Address::socket(sa.ip(), sa.port()));
    }
    if let Ok(ip) = s.parse::<IpAddr>() {
        return Some(Address::ip(ip));
    }
    if let Some((host, port)) = s.rsplit_once(':') {
        if !host.contains(':') {
            if let Ok(port) = port.parse::<u16>() {
                return Some(Address {
                    ip: None,
                    port: Some(port),
                    hostname: Some(host.to_string()),
                });
            }
        }
    }
    Some(Address::host(s))
}

/// `source` / `destination`: o hop extremo + endereço principal da entidade
/// no grafo (cai para `hop.address` se a entidade não estiver no grafo) e o
/// método `is_private()`.
fn endpoint_table(lua: &Lua, hop: Option<&Hop>, graph: &Graph) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let entity = hop.and_then(|h| graph.entity(&h.entity));
    let address: Option<Address> = match entity.and_then(|e| e.primary_address()) {
        Some(a) => Some(a.clone()),
        None => hop.and_then(|h| parse_address(&h.address)),
    };
    if let Some(h) = hop {
        t.raw_set("entity", h.entity.as_str())?;
        t.raw_set("name", h.name.as_str())?;
        t.raw_set("kind", h.kind.label())?;
        t.raw_set("status", lower(h.status.label()))?;
        t.raw_set("address", h.address.as_str())?;
    }
    if let Some(e) = entity {
        t.raw_set("provider", e.provider.label())?;
    }
    t.raw_set(
        "ip",
        address.as_ref().and_then(|a| a.ip).map(|ip| ip.to_string()),
    )?;
    t.raw_set("port", address.as_ref().and_then(|a| a.port))?;
    t.raw_set(
        "hostname",
        address.as_ref().and_then(|a| a.hostname.clone()),
    )?;
    let private = address.as_ref().is_some_and(Address::is_private);
    t.raw_set("is_private", lua.create_function(move |_, ()| Ok(private))?)?;
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use peabiru_core::{
        Confidence, Edge, Entity, EntityKind, Evidence, EvidenceKind, Metrics, Provider, Relation,
    };
    use peabiru_discovery::demo;
    use peabiru_trace::{TraceEngine, TraceRequest};

    fn demo_trace() -> (Graph, TraceResult) {
        let graph = demo::graph();
        let trace = TraceEngine::trace(&graph, &demo::default_request()).unwrap();
        (graph, trace)
    }

    fn engine_with(sources: &[(&str, &str)]) -> RuleEngine {
        let mut engine = RuleEngine::new().unwrap();
        for (name, src) in sources {
            engine.load_source(name, src).unwrap();
        }
        engine
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn observed() -> Evidence {
        Evidence::new(
            EvidenceKind::Observed,
            "eBPF (node/test)",
            Provider::Demo,
            Confidence(95),
        )
    }

    /// client (privado) → nat-gw (kind Nat) → server (`dst_ip`), com RTT 150 ms
    /// e 7 retransmissões na última aresta.
    fn nat_graph(dst_ip: &str, drops: u32) -> (Graph, TraceRequest) {
        let mut g = Graph::new();
        g.upsert_entity(
            Entity::new("ip://10.0.0.5", "client", EntityKind::Host, Provider::Linux)
                .with_address(Address::socket(ip("10.0.0.5"), 40000))
                .with_evidence(observed()),
        );
        g.upsert_entity(
            Entity::new(
                "aws://1/natgw/nat-1",
                "nat-gw",
                EntityKind::Nat,
                Provider::Aws,
            )
            .with_address(Address::ip(ip("10.0.1.5")))
            .with_evidence(observed()),
        );
        g.upsert_entity(
            Entity::new("ip://server", "server", EntityKind::Host, Provider::Linux)
                .with_address(Address::socket(ip(dst_ip), 443))
                .with_evidence(observed()),
        );
        g.add_edge(
            Edge::new("ip://10.0.0.5", "aws://1/natgw/nat-1", Relation::RoutesTo)
                .with_evidence(observed()),
        );
        g.add_edge(
            Edge::new("aws://1/natgw/nat-1", "ip://server", Relation::Connects)
                .with_evidence(observed())
                .with_metrics(Metrics {
                    rtt_ms: Some(150.0),
                    retransmissions: 7,
                    drops,
                    rst: 0,
                }),
        );
        (g, TraceRequest::new("client", "server", 443, "HTTPS"))
    }

    fn messages(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.message.as_str()).collect()
    }

    #[test]
    fn sandbox_removes_dangerous_globals_and_keeps_safe_ones() {
        let engine = RuleEngine::new().unwrap();
        let globals = engine.lua.globals();
        for name in BLOCKED_GLOBALS {
            let v: Value = globals.raw_get(*name).unwrap();
            assert!(v.is_nil(), "global `{name}` must be nil in the sandbox");
        }
        for name in [
            "string", "table", "math", "utf8", "pairs", "ipairs", "tostring", "tonumber", "type",
            "select", "next", "error", "pcall", "assert",
        ] {
            let v: Value = globals.raw_get(name).unwrap();
            assert!(!v.is_nil(), "global `{name}` must be available");
        }
    }

    #[test]
    fn sandbox_is_visible_from_rules() {
        let (graph, trace) = demo_trace();
        let engine = engine_with(&[(
            "probe.lua",
            r#"
            if os ~= nil or io ~= nil or require ~= nil or load ~= nil or dofile ~= nil then
              error("sandbox leak")
            end
            finding(type(string.upper) .. "/" .. type(math.max) .. "/" .. type(pcall))
            "#,
        )]);
        let findings = engine.evaluate(&trace, &graph).unwrap();
        assert_eq!(messages(&findings), vec!["function/function/function"]);
    }

    #[test]
    fn default_rules_on_demo_trace_have_no_critical_findings() {
        let (graph, trace) = demo_trace();
        let mut engine = RuleEngine::new().unwrap();
        engine.load_default().unwrap();
        assert_eq!(engine.rule_count(), 1);
        assert_eq!(engine.rule_names(), vec![DEFAULT_RULES_NAME]);
        let findings = engine.evaluate(&trace, &graph).unwrap();
        assert!(
            findings.iter().all(|f| f.severity != Severity::Critical),
            "demo has no drops: {findings:?}"
        );
        assert!(findings.iter().all(|f| f.rule == DEFAULT_RULES_NAME));
        assert!(!findings.iter().any(|f| f.message.starts_with("rule error")));
        // Sem retransmissões, RTT de 1.217 ms e confiança 92: só a regra de
        // tromboning pode disparar (nat-gw é branch do caminho, origem e
        // destino privados).
        assert_eq!(messages(&findings), vec!["Possible traffic tromboning"]);
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn default_rules_fire_on_nat_path_between_private_endpoints() {
        let (graph, req) = nat_graph("10.0.2.9", 0);
        let trace = TraceEngine::trace(&graph, &req).unwrap();
        let mut engine = RuleEngine::new().unwrap();
        engine.load_default().unwrap();
        let findings = engine.evaluate(&trace, &graph).unwrap();
        let msgs = messages(&findings);
        assert!(msgs.contains(&"Possible traffic tromboning"), "{msgs:?}");
        assert!(msgs.contains(&"High TCP retransmission"), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.starts_with("High RTT")), "{msgs:?}");
        assert!(findings.iter().all(|f| f.severity == Severity::Warning));

        // Destino público: sem tromboning.
        let (graph, req) = nat_graph("52.95.23.10", 0);
        let trace = TraceEngine::trace(&graph, &req).unwrap();
        let findings = engine.evaluate(&trace, &graph).unwrap();
        assert!(!messages(&findings).contains(&"Possible traffic tromboning"));

        // Drops → critical.
        let (graph, req) = nat_graph("10.0.2.9", 3);
        let trace = TraceEngine::trace(&graph, &req).unwrap();
        let findings = engine.evaluate(&trace, &graph).unwrap();
        assert!(findings
            .iter()
            .any(|f| f.severity == Severity::Critical && f.message.contains("drops")));
    }

    #[test]
    fn low_confidence_rule_is_info() {
        let (graph, req) = nat_graph("10.0.2.9", 0);
        let mut trace = TraceEngine::trace(&graph, &req).unwrap();
        trace.summary.confidence = Confidence(40);
        let mut engine = RuleEngine::new().unwrap();
        engine.load_default().unwrap();
        let findings = engine.evaluate(&trace, &graph).unwrap();
        let low = findings
            .iter()
            .find(|f| f.message == "Low path confidence")
            .unwrap();
        assert_eq!(low.severity, Severity::Info);
    }

    #[test]
    fn rule_error_becomes_a_warning_and_other_rules_still_run() {
        let (graph, trace) = demo_trace();
        let engine = engine_with(&[
            ("first.lua", "finding('first')"),
            (
                "broken.lua",
                "local x = nil\nlocal y = x + 1\nfinding('unreachable')",
            ),
            ("last.lua", "finding('last', 'info')"),
        ]);
        let findings = engine.evaluate(&trace, &graph).unwrap();
        assert_eq!(findings.len(), 3);
        assert_eq!(findings[0].message, "first");
        assert_eq!(findings[1].rule, "broken.lua");
        assert_eq!(findings[1].severity, Severity::Warning);
        assert!(
            findings[1].message.starts_with("rule error: ")
                && findings[1].message.contains("broken.lua:2:"),
            "{}",
            findings[1].message
        );
        assert!(!findings[1].message.contains('\n'));
        assert_eq!(findings[2].message, "last");
        assert_eq!(findings[2].severity, Severity::Info);
    }

    #[test]
    fn finding_severity_parsing_and_subject() {
        let (graph, trace) = demo_trace();
        let engine = engine_with(&[(
            "sev.lua",
            r#"
            finding("x", "critical")
            finding("y", "INFO")
            finding("z", "bogus")
            finding("w")
            finding("s", "warning", "aws-alb")
            finding("t", nil, "ip://10.0.3.8")
            finding("u", "info", "somewhere-else")
            "#,
        )]);
        let findings = engine.evaluate(&trace, &graph).unwrap();
        let sev: Vec<Severity> = findings.iter().map(|f| f.severity).collect();
        assert_eq!(
            sev,
            vec![
                Severity::Critical,
                Severity::Info,
                Severity::Warning,
                Severity::Warning,
                Severity::Warning,
                Severity::Warning,
                Severity::Info
            ]
        );
        assert!(findings.iter().all(|f| f.rule == "sev.lua"));
        assert_eq!(findings[4].subject.as_deref(), Some(demo::AWS_ALB));
        assert_eq!(findings[5].subject.as_deref(), Some(demo::REDIS));
        assert_eq!(findings[6].subject.as_deref(), Some("somewhere-else"));
        assert_eq!(findings[0].subject, None);
    }

    #[test]
    fn finding_without_message_is_a_rule_error() {
        let (graph, trace) = demo_trace();
        let engine = engine_with(&[("nomsg.lua", "finding()")]);
        let findings = engine.evaluate(&trace, &graph).unwrap();
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0].message.starts_with("rule error: "),
            "{}",
            findings[0].message
        );
    }

    #[test]
    fn api_tables_reflect_the_demo_trace() {
        let (graph, trace) = demo_trace();
        let engine = engine_with(&[(
            "api.lua",
            r#"
            finding(string.format("%d|%s|%s|%s|%s|%s|%s|%.3f|%d|%d|%d",
              #path.hops, path.hops[1].name, path.hops[4].kind, path.hops[4].status,
              path.branches[1].name, source.ip, destination.ip, flow.rtt_ms,
              trace.confidence, trace.hops, trace.duration_ms))
            finding(string.format("%s|%s|%d|%s|%s|%s",
              path.hops[2].edge_source, path.hops[1].edge_source == nil and "nil" or "set",
              path.hops[5].index, destination.name, destination.kind, trace.request.to))
            finding(string.format("%s|%s|%s|%s|%d|%s",
              tostring(source:is_private()), tostring(destination:is_private()),
              tostring(path:contains("NAT")), tostring(path:contains("aws-alb")),
              path.branches[1].anchor, path.branches[1].direction))
            finding(string.format("%s|%s|%s|%s",
              tostring(path.contains("pod")), tostring(path:contains("firewall")),
              tostring(path:contains(nil)), trace.status))
            "#,
        )]);
        let findings = engine.evaluate(&trace, &graph).unwrap();
        let msgs = messages(&findings);
        assert_eq!(
            msgs[0],
            format!(
                "5|web-frontend-7f9c8c|load_balancer|correlated|nat-gw|10.42.1.15|10.0.2.34|1.217|{}|5|{}",
                trace.summary.confidence.pct(),
                demo::DEMO_DURATION_MS
            )
        );
        assert_eq!(
            msgs[1],
            "eBPF (node/ip-10-42-1-1)|nil|5|app-svc|service|api.prod.svc.cluster.local"
        );
        assert_eq!(msgs[2], "true|true|true|true|3|outgoing");
        assert_eq!(msgs[3], "true|false|false|completed");
    }

    #[test]
    fn each_rule_gets_its_own_environment() {
        let (graph, trace) = demo_trace();
        let engine = engine_with(&[
            ("a.lua", "shared = 1; _G.leak = 2"),
            (
                "b.lua",
                "if shared == nil and leak == nil then finding('isolated') end",
            ),
            (
                "c.lua",
                "counter = (counter or 0) + 1; finding('count ' .. counter)",
            ),
        ]);
        let first = engine.evaluate(&trace, &graph).unwrap();
        assert_eq!(messages(&first), vec!["isolated", "count 1"]);
        let second = engine.evaluate(&trace, &graph).unwrap();
        assert_eq!(messages(&second), vec!["isolated", "count 1"]);
        let leak: Value = engine.lua.globals().raw_get("shared").unwrap();
        assert!(leak.is_nil());
    }

    #[test]
    fn load_errors_and_file_loading() {
        let mut engine = RuleEngine::new().unwrap();
        assert!(engine.load_source("bad.lua", "if then").is_err());
        assert!(engine
            .load_file(Path::new("/nonexistent/peabiru/x.lua"))
            .is_err());
        assert_eq!(engine.rule_count(), 0);

        let dir = std::env::temp_dir().join(format!("peabiru-lua-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("custom.lua");
        std::fs::write(&file, "finding('from file', 'critical')").unwrap();
        engine.load_file(&file).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(engine.rule_names(), vec!["custom.lua"]);

        let (graph, trace) = demo_trace();
        let findings = engine.evaluate(&trace, &graph).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "custom.lua");
        assert_eq!(findings[0].severity, Severity::Critical);
        assert_eq!(findings[0].message, "from file");
    }

    #[test]
    fn evaluate_without_rules_is_empty() {
        let (graph, trace) = demo_trace();
        let engine = RuleEngine::new().unwrap();
        assert!(engine.evaluate(&trace, &graph).unwrap().is_empty());
    }

    #[test]
    fn endpoint_falls_back_to_hop_address_when_entity_is_missing() {
        let (graph, req) = nat_graph("10.0.2.9", 0);
        let trace = TraceEngine::trace(&graph, &req).unwrap();
        let empty = Graph::new();
        let engine = engine_with(&[(
            "ep.lua",
            "finding(tostring(source.ip) .. '|' .. tostring(source:is_private()) .. '|' .. tostring(destination.port))",
        )]);
        let findings = engine.evaluate(&trace, &empty).unwrap();
        assert_eq!(messages(&findings), vec!["10.0.0.5|true|443"]);
    }

    #[test]
    fn parse_address_forms() {
        assert_eq!(parse_address(""), None);
        assert_eq!(
            parse_address("10.0.2.34:443"),
            Some(Address::socket(ip("10.0.2.34"), 443))
        );
        assert_eq!(parse_address("10.0.1.5"), Some(Address::ip(ip("10.0.1.5"))));
        assert_eq!(
            parse_address("[::1]:443"),
            Some(Address::socket(ip("::1"), 443))
        );
        assert_eq!(
            parse_address("svc:8080"),
            Some(Address {
                ip: None,
                port: Some(8080),
                hostname: Some("svc".into()),
            })
        );
        assert_eq!(
            parse_address("alb2c3.elb.amazonaws.com"),
            Some(Address::host("alb2c3.elb.amazonaws.com"))
        );
    }
}
