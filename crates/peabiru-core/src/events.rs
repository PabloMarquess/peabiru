//! Eventos emitidos por sensors/correlação/inferência (linha do painel EVENTS).

use serde::{Deserialize, Serialize};

use crate::model::Timestamp;
use crate::status::NodeStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventSource {
    Ebpf,
    FlowLog,
    Correlation,
    Inference,
    Probe,
    Discovery,
}

impl EventSource {
    /// `"eBPF"` `"flow_log"` `"correlation"` `"inference"` `"probe"` `"discovery"`.
    pub fn label(&self) -> &'static str {
        match self {
            EventSource::Ebpf => "eBPF",
            EventSource::FlowLog => "flow_log",
            EventSource::Correlation => "correlation",
            EventSource::Inference => "inference",
            EventSource::Probe => "probe",
            EventSource::Discovery => "discovery",
        }
    }

    /// `Ebpf | FlowLog | Probe` → `Observed`, `Correlation` → `Correlated`,
    /// `Inference | Discovery` → `Inferred`.
    pub fn status(&self) -> NodeStatus {
        match self {
            EventSource::Ebpf | EventSource::FlowLog | EventSource::Probe => NodeStatus::Observed,
            EventSource::Correlation => NodeStatus::Correlated,
            EventSource::Inference | EventSource::Discovery => NodeStatus::Inferred,
        }
    }
}

/// Linha de evento já formatada para exibição.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub ts: Timestamp,
    pub source: EventSource,
    /// `"connect"` | `"match"` | `"accept"` | `"route"` | `"probe"` | `"resolve"`.
    pub action: String,
    /// Texto já formatado: `"web-frontend-7f9c8c:51542"`.
    pub from: String,
    /// `"kube-proxy:15001"`.
    pub to: String,
    /// `true` → seta `"↔"`.
    pub bidirectional: bool,
    /// `"1.217ms"` | `"92%"` | `"(inferred)"`.
    pub detail: String,
}

impl Event {
    /// `"↔"` se bidirecional, senão `"→"`.
    pub fn arrow(&self) -> &'static str {
        if self.bidirectional {
            "↔"
        } else {
            "→"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn source_labels_and_status() {
        assert_eq!(EventSource::Ebpf.label(), "eBPF");
        assert_eq!(EventSource::FlowLog.label(), "flow_log");
        assert_eq!(EventSource::Discovery.label(), "discovery");
        assert_eq!(EventSource::Ebpf.status(), NodeStatus::Observed);
        assert_eq!(EventSource::FlowLog.status(), NodeStatus::Observed);
        assert_eq!(EventSource::Probe.status(), NodeStatus::Observed);
        assert_eq!(EventSource::Correlation.status(), NodeStatus::Correlated);
        assert_eq!(EventSource::Inference.status(), NodeStatus::Inferred);
        assert_eq!(EventSource::Discovery.status(), NodeStatus::Inferred);
    }

    #[test]
    fn event_round_trip() {
        let ev = Event {
            ts: Utc::now(),
            source: EventSource::Correlation,
            action: "match".into(),
            from: "istio-ingressgateway".into(),
            to: "aws-alb".into(),
            bidirectional: true,
            detail: "92%".into(),
        };
        assert_eq!(ev.arrow(), "↔");
        let json = serde_json::to_string(&ev).unwrap();
        let back: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ev);
    }
}
