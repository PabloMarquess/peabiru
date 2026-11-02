//! Confiança de arestas e do caminho (VISION §16).

use peabiru_core::{Confidence, Edge};

use crate::result::Hop;

/// Confiança usada quando a aresta não tem nenhuma evidência.
pub const NO_EVIDENCE_CONFIDENCE: f64 = 0.5;

/// Confiança efetiva de uma aresta em `0.0..=1.0`: máximo de
/// `Evidence::effective_confidence()` entre as evidências; `0.5` se a aresta
/// não tem evidência (o core devolve 0 nesse caso, e o fallback é política do
/// trace engine).
pub fn edge_confidence(edge: &Edge) -> f64 {
    edge.evidence
        .iter()
        .map(|e| e.effective_confidence())
        .fold(None, |best: Option<f64>, c| {
            Some(best.map_or(c, |b| b.max(c)))
        })
        .unwrap_or(NO_EVIDENCE_CONFIDENCE)
        .clamp(0.0, 1.0)
}

/// Confiança fim a fim do caminho: média geométrica das confianças das
/// arestas de entrada (`hops[1..]`; o hop 0 não tem aresta de entrada),
/// arredondada. Caminho de um único hop (sem arestas) → `100`.
pub fn path_confidence(hops: &[Hop]) -> Confidence {
    let ratios: Vec<f64> = hops.iter().skip(1).map(|h| h.confidence.ratio()).collect();
    if ratios.is_empty() {
        return Confidence::VERIFIED;
    }
    if ratios.iter().any(|&r| r <= 0.0) {
        return Confidence(0);
    }
    let mean_log = ratios.iter().map(|r| r.ln()).sum::<f64>() / ratios.len() as f64;
    Confidence::from_ratio(mean_log.exp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use peabiru_core::{
        EntityKind, Evidence, EvidenceKind, Metrics, NodeStatus, Provider, Relation,
    };

    fn ev(kind: EvidenceKind, pct: u8) -> Evidence {
        Evidence::new(kind, "src", Provider::Demo, Confidence(pct))
    }

    fn hop(index: usize, pct: u8) -> Hop {
        Hop {
            index,
            entity: format!("e{index}"),
            name: format!("e{index}"),
            kind: EntityKind::Host,
            status: NodeStatus::Observed,
            address: String::new(),
            edge_in: None,
            edge_status: NodeStatus::Observed,
            edge_source: None,
            evidence_kind: None,
            sources: 0,
            confidence: Confidence(pct),
            metrics: Metrics::default(),
        }
    }

    #[test]
    fn edge_confidence_uses_best_effective_and_fallback() {
        let bare = Edge::new("a", "b", Relation::Connects);
        assert!((edge_confidence(&bare) - 0.5).abs() < 1e-9);
        let e = bare
            .with_evidence(ev(EvidenceKind::Inferred, 100)) // 0.70
            .with_evidence(ev(EvidenceKind::LogConfirmed, 90)) // 0.855
            .with_evidence(ev(EvidenceKind::Observed, 80)); // 0.80
        assert!((edge_confidence(&e) - 0.855).abs() < 1e-9);
    }

    #[test]
    fn path_confidence_is_geometric_mean_excluding_first_hop() {
        assert_eq!(path_confidence(&[]), Confidence(100));
        assert_eq!(path_confidence(&[hop(0, 10)]), Confidence(100));
        // sqrt(0.81 * 0.49) = 0.63
        assert_eq!(
            path_confidence(&[hop(0, 5), hop(1, 81), hop(2, 49)]),
            Confidence(63)
        );
        assert_eq!(
            path_confidence(&[hop(0, 100), hop(1, 100), hop(2, 0)]),
            Confidence(0)
        );
        assert_eq!(
            path_confidence(&[hop(0, 100), hop(1, 92), hop(2, 92)]),
            Confidence(92)
        );
    }
}
