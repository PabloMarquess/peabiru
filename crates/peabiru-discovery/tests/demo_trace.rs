//! Trace de ponta a ponta sobre a fixture demo (ARCHITECTURE §4.1).

use peabiru_core::{Graph, NodeStatus, Relation};
use peabiru_discovery::demo::{self, DemoProvider};
use peabiru_discovery::Provider;
use peabiru_trace::{Direction, TraceEngine, TraceStatus};

#[test]
fn demo_trace_reproduces_the_mockup() {
    let mut graph = Graph::new();
    let report = DemoProvider.discover(&mut graph).unwrap();
    assert_eq!(report.entities, 10);
    assert_eq!(report.edges, 12);

    let req = demo::default_request();
    let trace = TraceEngine::trace(&graph, &req).unwrap();
    assert_eq!(trace.status, TraceStatus::Completed);
    assert_eq!(trace.request, req);
    assert_eq!(trace.request.to, "api.prod.svc.cluster.local");

    let hops: Vec<&str> = trace.hops.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(
        hops,
        vec![
            "web-frontend-7f9c8c",
            "kube-proxy",
            "istio-ingressgateway",
            "aws-alb",
            "app-svc"
        ]
    );
    for (i, h) in trace.hops.iter().enumerate() {
        assert_eq!(h.index, i);
        assert_eq!(h.edge_in.is_some(), i > 0);
    }
    let alb = trace.hop_for(demo::AWS_ALB).unwrap();
    assert_eq!(alb.status, NodeStatus::Correlated);
    assert_eq!(alb.edge_status, NodeStatus::Correlated);
    assert_eq!(alb.address, "alb2c3.elb.amazonaws.com");
    assert_eq!(alb.edge_source.as_deref(), Some("eBPF (node/ip-10-0-1-25)"));
    assert_eq!(alb.metrics.rtt_ms, Some(1.103));
    assert_eq!(alb.metrics.retransmissions, 2, "aresta 3 retransmite 2x");
    assert_eq!(trace.total_retransmissions(), 2);
    assert_eq!(trace.total_drops(), 0);
    assert_eq!(trace.total_rst(), 0);
    assert_eq!(
        trace.hops[1].edge_source.as_deref(),
        Some("eBPF (node/ip-10-42-1-1)")
    );
    assert_eq!(trace.hops[4].edge_status, NodeStatus::Observed);
    assert_eq!(trace.hops[4].edge_source.as_deref(), Some("VPC Flow Logs"));

    let branches: Vec<(&str, usize)> = trace
        .branches
        .iter()
        .map(|b| (b.name.as_str(), b.anchor))
        .collect();
    // Ordem do mockup: orders-db (correlada) sob o app-svc, depois redis e
    // payments-svc. Essa ordem vem da inserção das arestas na fixture.
    assert_eq!(
        branches,
        vec![
            ("nat-gw", 2),
            ("orders-db", 4),
            ("redis", 4),
            ("payments-svc", 4)
        ]
    );
    let by_name = |n: &str| trace.branches.iter().find(|b| b.name == n).unwrap();
    assert_eq!(by_name("nat-gw").status, NodeStatus::Inferred);
    assert_eq!(by_name("nat-gw").edge_status, NodeStatus::Inferred);
    assert_eq!(by_name("nat-gw").direction, Direction::Outgoing);
    assert_eq!(by_name("orders-db").status, NodeStatus::Correlated);
    assert_eq!(by_name("orders-db").edge_status, NodeStatus::Inferred);
    assert_eq!(by_name("redis").status, NodeStatus::Inferred);
    assert_eq!(by_name("payments-svc").status, NodeStatus::Inferred);
    assert!(trace
        .branches
        .iter()
        .all(|b| b.direction == Direction::Outgoing));

    let scores: Vec<u8> = trace.correlations.iter().map(|c| c.score.pct()).collect();
    assert_eq!(scores, vec![92, 81, 73]);
    assert_eq!(trace.correlations[0].a_name, "aws-alb");
    assert_eq!(trace.correlations[0].b_name, "app-svc");
    assert_eq!(trace.correlations[0].basis, "VPC Flow Logs + eBPF 5-tuple");
    assert_eq!(trace.correlations[1].a_name, "web-frontend-7f9c8c");
    assert_eq!(trace.correlations[1].b_name, "orders-db");
    assert_eq!(trace.correlations[1].basis, "cgroup + flow log");
    assert_eq!(trace.correlations[2].a_name, "istio-ingressgateway");
    assert_eq!(trace.correlations[2].b_name, "redis");
    assert_eq!(trace.correlations[2].basis, "timing correlation");
    // Nada correlacionado é exibido como observado: toda aresta Correlated
    // junta duas fontes e sai com status Correlated (`pb graph`, Topology).
    for (_, e) in graph
        .edges()
        .filter(|(_, e)| e.relation == Relation::Correlated)
    {
        assert_eq!(
            e.status(),
            NodeStatus::Correlated,
            "{} <-> {}",
            e.from,
            e.to
        );
        assert!(e.evidence.len() >= 2, "{} <-> {}", e.from, e.to);
    }

    let s = &trace.summary;
    assert_eq!(s.hops, 5);
    assert_eq!(s.correlations, 3);
    assert_eq!(s.observed + s.inferred, s.hops);
    assert!(
        (80..=100).contains(&s.confidence.pct()),
        "confidence {}",
        s.confidence
    );
    assert_eq!(s.duration_ms, demo::DEMO_DURATION_MS);
    assert!(trace.findings.is_empty());

    let text = trace.render_ascii();
    assert!(text.contains("PEABIRU TRACE"));
    assert!(text.contains("aws-alb"));
    assert!(
        text.contains("│ OBSERVED (correlated, 2 sources)\n"),
        "{text}"
    );
    assert!(text.contains("│ LOG_CONFIRMED\n"), "{text}");
    assert!(text.contains("│ eBPF (node/ip-10-0-1-25)\n"));
    assert!(text.contains("TCP RTT:             1.217 ms"));
    assert!(text.contains("Retransmissions:     2\n"), "{text}");
    assert!(text.contains(&format!("Path confidence:     {}%", s.confidence.pct())));

    let json = serde_json::to_string_pretty(&trace).unwrap();
    let back: peabiru_trace::TraceResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back, trace);
}
