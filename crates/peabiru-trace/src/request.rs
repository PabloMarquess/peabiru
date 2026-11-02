//! Pedido de trace: origem, destino, porta e protocolo.

use serde::{Deserialize, Serialize};

/// O que o usuário pediu: `peabiru trace --from A --to B --port 443`.
///
/// `from`/`to` são selectors aceitos por `Graph::resolve` (id, nome, `ip`,
/// `ip:port`, hostname). `to` pode ser um FQDN lógico como
/// `api.prod.svc.cluster.local`; nesse caso o engine segue `ResolvesTo`
/// para achar o destino real, mas `to` continua com o FQDN.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceRequest {
    /// Selector da origem.
    pub from: String,
    /// Selector ou FQDN do destino.
    pub to: String,
    /// Porta de destino.
    pub port: u16,
    /// `"HTTPS"` | `"TCP"` | `"HTTP"` — por enquanto só rótulo.
    pub protocol: String,
}

impl TraceRequest {
    pub fn new(from: impl Into<String>, to: impl Into<String>, port: u16, protocol: &str) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            port,
            protocol: protocol.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_and_serde() {
        let r = TraceRequest::new("web", "api.prod.svc.cluster.local", 443, "HTTPS");
        assert_eq!(r.from, "web");
        assert_eq!(r.port, 443);
        let json = serde_json::to_string(&r).unwrap();
        let back: TraceRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }
}
