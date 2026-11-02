//! Evidência e confiança (VISION §15–16). Toda relação e entidade carrega
//! `Vec<Evidence>`; nada inferido é exibido como observado.

use std::collections::BTreeMap;
use std::fmt;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::model::{Provider, Timestamp};

/// Como uma relação/entidade foi descoberta.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EvidenceKind {
    Observed,
    Inferred,
    ActiveTest,
    LogConfirmed,
    Discovered,
}

impl EvidenceKind {
    /// `"OBSERVED"`, `"INFERRED"`, `"ACTIVE_TEST"`, `"LOG_CONFIRMED"`, `"DISCOVERED"`.
    pub fn label(&self) -> &'static str {
        match self {
            EvidenceKind::Observed => "OBSERVED",
            EvidenceKind::Inferred => "INFERRED",
            EvidenceKind::ActiveTest => "ACTIVE_TEST",
            EvidenceKind::LogConfirmed => "LOG_CONFIRMED",
            EvidenceKind::Discovered => "DISCOVERED",
        }
    }

    /// `Observed | ActiveTest | LogConfirmed` → `true`.
    pub fn is_observational(&self) -> bool {
        matches!(
            self,
            EvidenceKind::Observed | EvidenceKind::ActiveTest | EvidenceKind::LogConfirmed
        )
    }

    /// Peso da fonte no cálculo de confiança efetiva.
    pub fn weight(&self) -> f64 {
        match self {
            EvidenceKind::Observed => 1.0,
            EvidenceKind::LogConfirmed => 0.95,
            EvidenceKind::ActiveTest => 0.9,
            EvidenceKind::Discovered => 0.85,
            EvidenceKind::Inferred => 0.7,
        }
    }
}

/// Confiança em percentual, `0..=100`.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Confidence(pub u8);

impl Confidence {
    pub const VERIFIED: Confidence = Confidence(100);

    /// Clamp em 100.
    pub fn new(pct: u8) -> Self {
        Self(pct.min(100))
    }

    /// De uma razão `0.0..=1.0` (clamp; `NaN` → 0), arredondada.
    pub fn from_ratio(r: f64) -> Self {
        let r = if r.is_nan() { 0.0 } else { r.clamp(0.0, 1.0) };
        Self((r * 100.0).round() as u8)
    }

    pub fn pct(&self) -> u8 {
        self.0.min(100)
    }

    pub fn ratio(&self) -> f64 {
        f64::from(self.pct()) / 100.0
    }

    /// `>=95` `"VERIFIED"`, `>=80` `"HIGH"`, `>=60` `"MEDIUM"`, senão `"LOW"`.
    pub fn label(&self) -> &'static str {
        match self.pct() {
            95..=100 => "VERIFIED",
            80..=94 => "HIGH",
            60..=79 => "MEDIUM",
            _ => "LOW",
        }
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.pct())
    }
}

/// Um fato sobre uma entidade ou relação: quem viu, quando, com que confiança.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    /// `"eBPF (node/ip-10-0-1-25)"`, `"VPC Flow Logs"`, `"AWS Route Table"`, `"Kubernetes API"`.
    pub source: String,
    pub provider: Provider,
    pub timestamp: Timestamp,
    pub confidence: Confidence,
    pub metadata: BTreeMap<String, String>,
}

impl Evidence {
    /// `timestamp = Utc::now()`.
    pub fn new(
        kind: EvidenceKind,
        source: &str,
        provider: Provider,
        confidence: Confidence,
    ) -> Self {
        Self {
            kind,
            source: source.to_string(),
            provider,
            timestamp: Utc::now(),
            confidence,
            metadata: BTreeMap::new(),
        }
    }

    pub fn at(mut self, t: Timestamp) -> Self {
        self.timestamp = t;
        self
    }

    pub fn with_meta(mut self, k: &str, v: &str) -> Self {
        self.metadata.insert(k.to_string(), v.to_string());
        self
    }

    /// `kind.weight() * confidence.ratio()`.
    pub fn effective_confidence(&self) -> f64 {
        self.kind.weight() * self.confidence.ratio()
    }
}

/// Fontes distintas de evidência observacional, na ordem em que aparecem.
pub fn observational_sources(evidence: &[Evidence]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in evidence.iter().filter(|e| e.kind.is_observational()) {
        if !out.iter().any(|s| s == &e.source) {
            out.push(e.source.clone());
        }
    }
    out
}

/// Anexa `from` em `into` sem duplicar `(kind, source)`. Quando já existe uma
/// entrada equivalente, ela é atualizada com o timestamp/confiança da mais
/// recente e os metadados são mesclados — nunca há duas entradas iguais.
pub(crate) fn merge_evidence(into: &mut Vec<Evidence>, from: impl IntoIterator<Item = Evidence>) {
    for e in from {
        match into
            .iter_mut()
            .find(|x| x.kind == e.kind && x.source == e.source)
        {
            Some(existing) => {
                if e.timestamp >= existing.timestamp {
                    existing.timestamp = e.timestamp;
                    existing.confidence = e.confidence;
                }
                existing.metadata.extend(e.metadata);
            }
            None => into.push(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn confidence_label_thresholds() {
        assert_eq!(Confidence(100).label(), "VERIFIED");
        assert_eq!(Confidence(95).label(), "VERIFIED");
        assert_eq!(Confidence(94).label(), "HIGH");
        assert_eq!(Confidence(80).label(), "HIGH");
        assert_eq!(Confidence(79).label(), "MEDIUM");
        assert_eq!(Confidence(60).label(), "MEDIUM");
        assert_eq!(Confidence(59).label(), "LOW");
        assert_eq!(Confidence(0).label(), "LOW");
        assert_eq!(Confidence::VERIFIED, Confidence(100));
    }

    #[test]
    fn confidence_construction_and_ratio() {
        assert_eq!(Confidence::new(250), Confidence(100));
        assert_eq!(Confidence(250).pct(), 100);
        assert_eq!(Confidence::from_ratio(0.92), Confidence(92));
        assert_eq!(Confidence::from_ratio(0.925), Confidence(93));
        assert_eq!(Confidence::from_ratio(1.7), Confidence(100));
        assert_eq!(Confidence::from_ratio(-0.3), Confidence(0));
        assert_eq!(Confidence::from_ratio(f64::NAN), Confidence(0));
        assert!(approx(Confidence(92).ratio(), 0.92));
        assert_eq!(Confidence(92).to_string(), "92%");
        assert!(Confidence(80) > Confidence(79));
    }

    #[test]
    fn evidence_kind_weights_and_labels() {
        assert!(approx(EvidenceKind::Observed.weight(), 1.0));
        assert!(approx(EvidenceKind::LogConfirmed.weight(), 0.95));
        assert!(approx(EvidenceKind::ActiveTest.weight(), 0.9));
        assert!(approx(EvidenceKind::Discovered.weight(), 0.85));
        assert!(approx(EvidenceKind::Inferred.weight(), 0.7));
        assert!(EvidenceKind::Observed.is_observational());
        assert!(EvidenceKind::ActiveTest.is_observational());
        assert!(EvidenceKind::LogConfirmed.is_observational());
        assert!(!EvidenceKind::Discovered.is_observational());
        assert!(!EvidenceKind::Inferred.is_observational());
        assert_eq!(EvidenceKind::ActiveTest.label(), "ACTIVE_TEST");
        assert_eq!(EvidenceKind::LogConfirmed.label(), "LOG_CONFIRMED");
    }

    #[test]
    fn effective_confidence_is_weighted() {
        let e = Evidence::new(
            EvidenceKind::Inferred,
            "route inference",
            Provider::Demo,
            Confidence(50),
        );
        assert!(approx(e.effective_confidence(), 0.35));
        let e = e.with_meta("k", "v");
        assert_eq!(e.metadata.get("k").map(String::as_str), Some("v"));
    }

    #[test]
    fn merge_evidence_dedups_by_kind_and_source() {
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t1 = t0 + Duration::seconds(10);
        let mut into = vec![Evidence::new(
            EvidenceKind::Observed,
            "eBPF",
            Provider::Demo,
            Confidence(90),
        )
        .at(t0)];
        merge_evidence(
            &mut into,
            vec![
                Evidence::new(
                    EvidenceKind::Observed,
                    "eBPF",
                    Provider::Demo,
                    Confidence(95),
                )
                .at(t1)
                .with_meta("pid", "42"),
                Evidence::new(
                    EvidenceKind::LogConfirmed,
                    "eBPF",
                    Provider::Demo,
                    Confidence(80),
                ),
                Evidence::new(
                    EvidenceKind::Observed,
                    "VPC Flow Logs",
                    Provider::Aws,
                    Confidence(80),
                ),
            ],
        );
        assert_eq!(into.len(), 3);
        assert_eq!(into[0].timestamp, t1);
        assert_eq!(into[0].confidence, Confidence(95));
        assert_eq!(into[0].metadata.get("pid").map(String::as_str), Some("42"));
        assert_eq!(
            observational_sources(&into),
            vec!["eBPF".to_string(), "VPC Flow Logs".to_string()]
        );
    }
}
