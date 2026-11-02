//! Findings produzidos pelo diagnostic engine (regras Lua, VISION §23).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::model::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    /// `"info"` | `"warning"` | `"critical"`.
    pub fn label(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Critical => "critical",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Erro de `Severity::from_str`.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown severity {0:?} (expected info, warning or critical)")]
pub struct ParseSeverityError(pub String);

impl FromStr for Severity {
    type Err = ParseSeverityError;

    /// Case-insensitive: `"info"`, `"warning"` (ou `"warn"`), `"critical"` (ou `"crit"`).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "info" => Ok(Severity::Info),
            "warning" | "warn" => Ok(Severity::Warning),
            "critical" | "crit" => Ok(Severity::Critical),
            _ => Err(ParseSeverityError(s.to_string())),
        }
    }
}

/// Resultado de uma regra de diagnóstico sobre um trace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub message: String,
    /// Nome da regra/arquivo que gerou o finding.
    pub rule: String,
    /// Entidade a que o finding se refere, se houver.
    pub subject: Option<EntityId>,
}

impl Finding {
    pub fn new(severity: Severity, rule: &str, message: &str) -> Self {
        Self {
            severity,
            message: message.to_string(),
            rule: rule.to_string(),
            subject: None,
        }
    }

    pub fn with_subject(mut self, id: impl Into<EntityId>) -> Self {
        self.subject = Some(id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_parse_and_order() {
        assert_eq!("info".parse::<Severity>(), Ok(Severity::Info));
        assert_eq!(" Warning ".parse::<Severity>(), Ok(Severity::Warning));
        assert_eq!("CRITICAL".parse::<Severity>(), Ok(Severity::Critical));
        assert!("fatal".parse::<Severity>().is_err());
        assert_eq!(
            "fatal".parse::<Severity>().unwrap_err().to_string(),
            "unknown severity \"fatal\" (expected info, warning or critical)"
        );
        assert!(Severity::Critical > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
        assert_eq!(Severity::Warning.to_string(), "warning");
    }

    #[test]
    fn finding_builder_and_serde() {
        let f = Finding::new(
            Severity::Warning,
            "default.lua",
            "Possible traffic tromboning",
        )
        .with_subject("aws://1/natgw/nat-0a1b");
        assert_eq!(f.subject.as_deref(), Some("aws://1/natgw/nat-0a1b"));
        let json = serde_json::to_string(&f).unwrap();
        assert!(json.contains("\"severity\":\"Warning\""));
        let back: Finding = serde_json::from_str(&json).unwrap();
        assert_eq!(back, f);
    }
}
