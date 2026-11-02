//! Resultado de um trace: hops do caminho principal, branches, correlações,
//! resumo e findings — mais a renderização texto (VISION §24).

use std::fmt::Write as _;

use peabiru_core::{
    Confidence, EdgeId, EntityId, EntityKind, EvidenceKind, Finding, Metrics, NodeStatus, Timestamp,
};
use serde::{Deserialize, Serialize};

use crate::request::TraceRequest;

/// Estado de um trace (o engine sempre devolve `Completed`; `Idle`/`Running`
/// são usados pela UI durante o replay de eventos).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceStatus {
    Idle,
    Running,
    Completed,
    Failed,
}

impl TraceStatus {
    /// `"IDLE"` | `"RUNNING"` | `"COMPLETED"` | `"FAILED"`.
    pub fn label(&self) -> &'static str {
        match self {
            TraceStatus::Idle => "IDLE",
            TraceStatus::Running => "RUNNING",
            TraceStatus::Completed => "COMPLETED",
            TraceStatus::Failed => "FAILED",
        }
    }
}

/// Um nó do caminho principal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hop {
    /// Posição no caminho (0 = origem).
    pub index: usize,
    pub entity: EntityId,
    pub name: String,
    pub kind: EntityKind,
    /// Status da entidade (`Entity::status()`).
    pub status: NodeStatus,
    /// `Address::display()` do endereço principal (`""` se nenhum).
    pub address: String,
    /// Aresta que chega neste hop (`None` no primeiro).
    pub edge_in: Option<EdgeId>,
    /// Status da aresta de entrada (Observed → linha sólida, Inferred → tracejada).
    pub edge_status: NodeStatus,
    /// `source` da melhor evidência da aresta de entrada (`None` no primeiro
    /// hop ou se a aresta não tem evidência). Usado por `render_ascii`.
    #[serde(default)]
    pub edge_source: Option<String>,
    /// `kind` da melhor evidência da aresta de entrada (`None` no primeiro hop
    /// ou se a aresta não tem evidência). É o que `render_ascii` imprime como
    /// rótulo da aresta: nada inferido/confirmado por log aparece como OBSERVED.
    #[serde(default)]
    pub evidence_kind: Option<EvidenceKind>,
    /// Fontes observacionais distintas na aresta de entrada
    /// (`observational_sources(&edge.evidence).len()`; 0 no primeiro hop).
    /// `>= 2` é o que torna `edge_status == Correlated`.
    #[serde(default)]
    pub sources: usize,
    /// Confiança da aresta de entrada (100 no primeiro).
    pub confidence: Confidence,
    /// Métricas da aresta de entrada.
    pub metrics: Metrics,
}

impl Hop {
    /// Rótulo da aresta de entrada para `render_ascii`: o `label()` do
    /// `evidence_kind` (`OBSERVED`, `LOG_CONFIRMED`, `DISCOVERED`, ...), com
    /// ` (correlated, N sources)` quando `sources >= 2`. Sem evidência cai
    /// para `edge_status.label()`.
    pub fn edge_label(&self) -> String {
        let base = self
            .evidence_kind
            .map_or_else(|| self.edge_status.label(), |kind| kind.label());
        if self.sources >= 2 {
            format!("{base} (correlated, {} sources)", self.sources)
        } else {
            base.to_string()
        }
    }
}

/// Sentido da aresta que liga uma branch ao seu hop âncora.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// hop → branch
    Outgoing,
    /// branch → hop
    Incoming,
}

/// Vizinho de um hop que não está no caminho principal (nat-gw, redis, ...).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    /// Índice do `Hop` ao qual está ligada.
    pub anchor: usize,
    pub entity: EntityId,
    pub name: String,
    pub kind: EntityKind,
    pub status: NodeStatus,
    pub address: String,
    pub edge: EdgeId,
    pub edge_status: NodeStatus,
    pub direction: Direction,
}

/// Aresta `Relation::Correlated` relevante para o trace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Correlation {
    pub a: EntityId,
    pub a_name: String,
    pub b: EntityId,
    pub b_name: String,
    pub score: Confidence,
    /// `source` da melhor evidência: `"VPC Flow Logs + eBPF 5-tuple"`.
    pub basis: String,
}

/// Números do painel TRACE SUMMARY.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub hops: usize,
    /// Hops com `status.is_observed()`.
    pub observed: usize,
    /// Os demais hops.
    pub inferred: usize,
    pub correlations: usize,
    pub confidence: Confidence,
    /// Janela de observação do trace: intervalo entre a evidência mais antiga
    /// e a mais recente entre tudo que o trace tocou (2843 no demo).
    pub duration_ms: u64,
}

/// Resultado completo de `TraceEngine::trace`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceResult {
    pub id: u32,
    pub status: TraceStatus,
    pub request: TraceRequest,
    pub started_at: Timestamp,
    /// Caminho principal, ordem origem → destino.
    pub hops: Vec<Hop>,
    pub branches: Vec<Branch>,
    pub correlations: Vec<Correlation>,
    pub summary: Summary,
    /// Preenchido por `peabiru-lua` (vazio se sem regras).
    pub findings: Vec<Finding>,
}

impl TraceResult {
    /// Hop cujo `entity` é `id`.
    pub fn hop_for(&self, id: &str) -> Option<&Hop> {
        self.hops.iter().find(|h| h.entity == id)
    }

    /// Maior RTT entre as arestas do caminho.
    pub fn max_rtt_ms(&self) -> Option<f64> {
        self.hops
            .iter()
            .filter_map(|h| h.metrics.rtt_ms)
            .fold(None, |best: Option<f64>, r| {
                Some(best.map_or(r, |b| b.max(r)))
            })
    }

    /// Soma de retransmissões nas arestas do caminho.
    pub fn total_retransmissions(&self) -> u32 {
        self.hops.iter().map(|h| h.metrics.retransmissions).sum()
    }

    /// Soma de drops nas arestas do caminho.
    pub fn total_drops(&self) -> u32 {
        self.hops.iter().map(|h| h.metrics.drops).sum()
    }

    /// Soma de RSTs nas arestas do caminho.
    pub fn total_rst(&self) -> u32 {
        self.hops.iter().map(|h| h.metrics.rst).sum()
    }

    /// Saída texto no estilo de VISION §24 (para `peabiru trace` sem `--json`):
    ///
    /// ```text
    /// PEABIRU TRACE #1
    ///
    /// web-frontend-7f9c8c
    /// 10.42.1.15:51542
    ///
    ///      │
    ///      │ OBSERVED
    ///      │ eBPF (node/ip-10-42-1-1)
    ///      ▼
    ///
    /// kube-proxy
    /// ...
    ///      │
    ///      │ OBSERVED (correlated, 2 sources)
    ///      │ eBPF (node/ip-10-0-1-25)
    ///      ▼
    ///
    /// aws-alb
    /// alb2c3.elb.amazonaws.com
    ///
    ///      │
    ///      │ LOG_CONFIRMED
    ///      │ VPC Flow Logs
    ///      ▼
    ///
    /// app-svc
    /// ...
    ///
    /// Diagnostics:
    ///
    /// TCP RTT:             1.217 ms
    /// Retransmissions:     0
    /// Kernel drops:        0
    /// RST:                 0
    ///
    /// Path confidence:     92%
    /// ```
    ///
    /// O rótulo de cada aresta é `Hop::edge_label()` — o tipo da melhor
    /// evidência, não o status derivado — para nunca mostrar como OBSERVED o
    /// que foi só inferido, descoberto ou confirmado por log.
    pub fn render_ascii(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "PEABIRU TRACE #{}", self.id);
        out.push('\n');

        for (i, hop) in self.hops.iter().enumerate() {
            if i > 0 {
                out.push_str("     │\n");
                let _ = writeln!(out, "     │ {}", hop.edge_label());
                if let Some(src) = hop.edge_source.as_deref().filter(|s| !s.is_empty()) {
                    let _ = writeln!(out, "     │ {src}");
                }
                out.push_str("     ▼\n");
                out.push('\n');
            }
            out.push_str(&hop.name);
            out.push('\n');
            if !hop.address.is_empty() {
                out.push_str(&hop.address);
                out.push('\n');
            }
            out.push('\n');
        }

        out.push('\n');
        out.push_str("Diagnostics:\n\n");
        let rtt = self
            .max_rtt_ms()
            .map_or_else(|| "n/a".to_string(), |r| format!("{r:.3} ms"));
        let _ = writeln!(out, "{:<21}{}", "TCP RTT:", rtt);
        let _ = writeln!(
            out,
            "{:<21}{}",
            "Retransmissions:",
            self.total_retransmissions()
        );
        let _ = writeln!(out, "{:<21}{}", "Kernel drops:", self.total_drops());
        let _ = writeln!(out, "{:<21}{}", "RST:", self.total_rst());
        out.push('\n');
        let _ = writeln!(
            out,
            "{:<21}{}%",
            "Path confidence:",
            self.summary.confidence.pct()
        );

        if !self.findings.is_empty() {
            out.push('\n');
            out.push_str("Potential problem:\n\n");
            for f in &self.findings {
                let _ = writeln!(out, "[{}] {}", f.severity.label(), f.message);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use peabiru_core::Severity;

    fn hop(index: usize, name: &str, address: &str, source: Option<&str>, rtt: Option<f64>) -> Hop {
        Hop {
            index,
            entity: format!("id://{name}"),
            name: name.to_string(),
            kind: EntityKind::Host,
            status: NodeStatus::Observed,
            address: address.to_string(),
            edge_in: if index == 0 {
                None
            } else {
                Some(EdgeId(index - 1))
            },
            edge_status: if index == 0 {
                NodeStatus::Observed
            } else {
                NodeStatus::Inferred
            },
            edge_source: source.map(str::to_string),
            evidence_kind: if index == 0 {
                None
            } else {
                Some(EvidenceKind::Inferred)
            },
            sources: 0,
            confidence: Confidence(90),
            metrics: Metrics {
                rtt_ms: rtt,
                retransmissions: index as u32,
                drops: 0,
                rst: 1,
            },
        }
    }

    fn result() -> TraceResult {
        TraceResult {
            id: 84,
            status: TraceStatus::Completed,
            request: TraceRequest::new("a", "c", 443, "HTTPS"),
            started_at: Utc::now(),
            hops: vec![
                hop(0, "a", "10.0.0.1:1234", None, None),
                hop(1, "b", "10.0.0.2", Some("Route Table"), Some(2.5)),
                hop(2, "c", "", None, Some(1.0)),
            ],
            branches: vec![],
            correlations: vec![],
            summary: Summary {
                hops: 3,
                observed: 3,
                inferred: 0,
                correlations: 0,
                confidence: Confidence(89),
                duration_ms: 10,
            },
            findings: vec![Finding::new(
                Severity::Warning,
                "default.lua",
                "Retransmissions detected between source and destination.",
            )],
        }
    }

    #[test]
    fn hop_for_and_aggregates() {
        let r = result();
        assert_eq!(r.hop_for("id://b").map(|h| h.index), Some(1));
        assert!(r.hop_for("b").is_none());
        assert_eq!(r.max_rtt_ms(), Some(2.5));
        assert_eq!(r.total_retransmissions(), 3);
        assert_eq!(r.total_drops(), 0);
        assert_eq!(r.total_rst(), 3);
        assert_eq!(TraceStatus::Completed.label(), "COMPLETED");
    }

    #[test]
    fn render_ascii_layout() {
        let text = result().render_ascii();
        let expected = "PEABIRU TRACE #84\n\
\n\
a\n\
10.0.0.1:1234\n\
\n\
\x20    │\n\
\x20    │ INFERRED\n\
\x20    │ Route Table\n\
\x20    ▼\n\
\n\
b\n\
10.0.0.2\n\
\n\
\x20    │\n\
\x20    │ INFERRED\n\
\x20    ▼\n\
\n\
c\n\
\n\
\n\
Diagnostics:\n\
\n\
TCP RTT:             2.500 ms\n\
Retransmissions:     3\n\
Kernel drops:        0\n\
RST:                 3\n\
\n\
Path confidence:     89%\n\
\n\
Potential problem:\n\
\n\
[warning] Retransmissions detected between source and destination.\n";
        assert_eq!(text, expected);
    }

    #[test]
    fn render_ascii_without_metrics_or_findings() {
        let mut r = result();
        r.findings.clear();
        for h in &mut r.hops {
            h.metrics = Metrics::default();
        }
        let text = r.render_ascii();
        assert!(text.contains("TCP RTT:             n/a\n"));
        assert!(!text.contains("Potential problem"));
        let json = serde_json::to_string(&r).unwrap();
        let back: TraceResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn edge_label_uses_evidence_kind_not_derived_status() {
        // LogConfirmed único: status derivado é Observed, mas o rótulo diz LOG_CONFIRMED.
        let mut h = hop(1, "b", "", Some("VPC Flow Logs"), None);
        h.edge_status = NodeStatus::Observed;
        h.evidence_kind = Some(EvidenceKind::LogConfirmed);
        h.sources = 1;
        assert_eq!(h.edge_label(), "LOG_CONFIRMED");

        // Discovered: status derivado é Inferred, rótulo DISCOVERED.
        h.edge_status = NodeStatus::Inferred;
        h.evidence_kind = Some(EvidenceKind::Discovered);
        h.sources = 0;
        assert_eq!(h.edge_label(), "DISCOVERED");

        // Duas fontes observacionais: o melhor kind + o sufixo de correlação.
        h.edge_status = NodeStatus::Correlated;
        h.evidence_kind = Some(EvidenceKind::Observed);
        h.sources = 2;
        assert_eq!(h.edge_label(), "OBSERVED (correlated, 2 sources)");
        h.evidence_kind = Some(EvidenceKind::ActiveTest);
        h.sources = 3;
        assert_eq!(h.edge_label(), "ACTIVE_TEST (correlated, 3 sources)");

        // Sem evidência: cai para o status derivado.
        h.evidence_kind = None;
        h.sources = 0;
        h.edge_status = NodeStatus::Inferred;
        assert_eq!(h.edge_label(), "INFERRED");
    }

    #[test]
    fn render_ascii_prints_evidence_kind_and_correlation_suffix() {
        let mut r = result();
        r.findings.clear();
        r.hops[1].edge_status = NodeStatus::Correlated;
        r.hops[1].evidence_kind = Some(EvidenceKind::Observed);
        r.hops[1].sources = 2;
        r.hops[1].edge_source = Some("eBPF (node/ip-10-0-1-25)".into());
        r.hops[2].edge_status = NodeStatus::Observed;
        r.hops[2].evidence_kind = Some(EvidenceKind::LogConfirmed);
        r.hops[2].sources = 1;
        r.hops[2].edge_source = Some("VPC Flow Logs".into());
        let text = r.render_ascii();
        assert!(
            text.contains(
                "     │
     │ OBSERVED (correlated, 2 sources)
     │ eBPF (node/ip-10-0-1-25)
     ▼
"
            ),
            "{text}"
        );
        assert!(
            text.contains(
                "     │
     │ LOG_CONFIRMED
     │ VPC Flow Logs
     ▼
"
            ),
            "{text}"
        );
        assert!(
            !text.contains(
                "│ CORRELATED
"
            ),
            "{text}"
        );
        assert!(
            !text.contains(
                "│ OBSERVED
"
            ),
            "{text}"
        );
    }

    #[test]
    fn hop_json_carries_evidence_kind_and_sources_with_defaults() {
        let mut h = hop(1, "b", "", Some("eBPF"), None);
        h.evidence_kind = Some(EvidenceKind::LogConfirmed);
        h.sources = 2;
        let json = serde_json::to_value(&h).unwrap();
        assert_eq!(json["evidence_kind"], "LogConfirmed");
        assert_eq!(json["sources"], 2);

        // JSON antigo (sem os campos aditivos) continua desserializando.
        let mut old = json.clone();
        old.as_object_mut().unwrap().remove("evidence_kind");
        old.as_object_mut().unwrap().remove("sources");
        let back: Hop = serde_json::from_value(old).unwrap();
        assert_eq!(back.evidence_kind, None);
        assert_eq!(back.sources, 0);
    }
}
