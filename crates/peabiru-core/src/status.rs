//! Status derivado de entidades e arestas — o que a UI desenha (ARCHITECTURE §2.4).

use serde::{Deserialize, Serialize};

use crate::evidence::{observational_sources, Evidence};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeStatus {
    Observed,
    Inferred,
    Correlated,
    Probe,
}

impl NodeStatus {
    /// `"OBSERVED"` | `"INFERRED"` | `"CORRELATED"` | `"PROBE"`.
    pub fn label(&self) -> &'static str {
        match self {
            NodeStatus::Observed => "OBSERVED",
            NodeStatus::Inferred => "INFERRED",
            NodeStatus::Correlated => "CORRELATED",
            NodeStatus::Probe => "PROBE",
        }
    }

    /// `Observed | Correlated | Probe` → `true`.
    pub fn is_observed(&self) -> bool {
        !matches!(self, NodeStatus::Inferred)
    }

    /// Deriva o status só das evidências (sem a regra de `probe`):
    /// ≥ 2 fontes observacionais distintas → `Correlated`, ≥ 1 → `Observed`,
    /// senão `Inferred`.
    pub fn from_evidence(evidence: &[Evidence]) -> NodeStatus {
        match observational_sources(evidence).len() {
            0 => NodeStatus::Inferred,
            1 => NodeStatus::Observed,
            _ => NodeStatus::Correlated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::{Confidence, EvidenceKind};
    use crate::model::Provider;

    fn ev(kind: EvidenceKind, source: &str) -> Evidence {
        Evidence::new(kind, source, Provider::Demo, Confidence(80))
    }

    #[test]
    fn labels_and_is_observed() {
        assert_eq!(NodeStatus::Observed.label(), "OBSERVED");
        assert_eq!(NodeStatus::Inferred.label(), "INFERRED");
        assert_eq!(NodeStatus::Correlated.label(), "CORRELATED");
        assert_eq!(NodeStatus::Probe.label(), "PROBE");
        assert!(NodeStatus::Observed.is_observed());
        assert!(NodeStatus::Correlated.is_observed());
        assert!(NodeStatus::Probe.is_observed());
        assert!(!NodeStatus::Inferred.is_observed());
    }

    #[test]
    fn from_evidence_counts_distinct_observational_sources() {
        assert_eq!(NodeStatus::from_evidence(&[]), NodeStatus::Inferred);
        assert_eq!(
            NodeStatus::from_evidence(&[ev(EvidenceKind::Discovered, "Kubernetes API")]),
            NodeStatus::Inferred
        );
        assert_eq!(
            NodeStatus::from_evidence(&[
                ev(EvidenceKind::Observed, "eBPF"),
                ev(EvidenceKind::LogConfirmed, "eBPF")
            ]),
            NodeStatus::Observed
        );
        assert_eq!(
            NodeStatus::from_evidence(&[
                ev(EvidenceKind::Observed, "eBPF"),
                ev(EvidenceKind::ActiveTest, "tcp probe")
            ]),
            NodeStatus::Correlated
        );
    }
}
