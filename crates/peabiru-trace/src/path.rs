//! Path finding (ARCHITECTURE §3): Dijkstra sobre arestas de saída (mais as
//! `Peer`, que são simétricas e valem nos dois sentidos), custo
//! `1.0 + (1.0 - edge_confidence)`, desempate determinístico pela ordem de
//! inserção das arestas; resolução do destino via `ResolvesTo`; branches.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use peabiru_core::{Edge, EdgeId, EntityId, Graph, Relation};

use crate::confidence::edge_confidence;
use crate::result::{Branch, Direction, Hop};

/// Relações atravessadas pelo Dijkstra (`Contains`/`Correlated` ficam de fora).
pub fn is_traversable(relation: Relation) -> bool {
    matches!(
        relation,
        Relation::Connects
            | Relation::RoutesTo
            | Relation::ForwardsTo
            | Relation::ResolvesTo
            | Relation::Peer
    )
}

/// Relações simétricas: o Dijkstra as atravessa também de `to` para `from`,
/// em linha com a seta `↔` de `Relation::arrow()`. Só `Peer` (`Correlated`
/// também é `↔`, mas não é atravessável).
pub fn is_symmetric(relation: Relation) -> bool {
    matches!(relation, Relation::Peer)
}

/// Relações que ligam um hop a uma branch (qualquer uma exceto `Contains`/`Correlated`).
pub fn is_branch_relation(relation: Relation) -> bool {
    !matches!(relation, Relation::Contains | Relation::Correlated)
}

/// Custo de atravessar uma aresta: `1.0 + (1.0 - edge_confidence)`, em `1.0..=2.0`.
/// Com o mesmo número de hops, o caminho com mais confiança (observado) ganha.
pub fn edge_cost(edge: &Edge) -> f64 {
    1.0 + (1.0 - edge_confidence(edge))
}

/// Custo em micro-unidades inteiras: empates ficam exatos e determinísticos.
const COST_SCALE: f64 = 1_000_000.0;

fn cost_units(edge: &Edge) -> u64 {
    (edge_cost(edge) * COST_SCALE).round() as u64
}

/// Um passo do caminho: a entidade e a aresta pela qual se chegou nela.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathStep {
    pub entity: EntityId,
    /// `None` na origem.
    pub edge_in: Option<EdgeId>,
}

/// Destino efetivo do trace depois de seguir `ResolvesTo` (§4.1 item 5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceTarget {
    /// Entidade para a qual o selector resolveu diretamente (ex.: `dns://...`).
    pub selected: EntityId,
    /// Destino real do path finding (igual a `selected` se nada foi seguido).
    pub entity: EntityId,
    /// Arestas `ResolvesTo` seguidas, na ordem.
    pub resolution: Vec<EdgeId>,
}

impl TraceTarget {
    /// `true` quando o selector foi desembrulhado para outra entidade.
    pub fn was_unwrapped(&self) -> bool {
        self.selected != self.entity
    }
}

/// Resolve o selector de destino (`Graph::resolve`) e, se a entidade tiver
/// aresta `ResolvesTo` de saída para outra entidade existente, usa essa
/// entidade como destino real — repetidamente (cadeias CNAME), com proteção
/// contra ciclos. A primeira aresta `ResolvesTo` em ordem de inserção vence.
pub fn resolve_target(graph: &Graph, selector: &str) -> Option<TraceTarget> {
    let selected = graph.resolve(selector)?.id.clone();
    let (entity, resolution) = unwrap_resolves_to(graph, &selected);
    Some(TraceTarget {
        selected,
        entity,
        resolution,
    })
}

/// Segue `ResolvesTo` de saída a partir de `start` enquanto houver uma aresta
/// para uma entidade existente ainda não visitada. Devolve a entidade final
/// e as arestas percorridas.
pub fn unwrap_resolves_to(graph: &Graph, start: &str) -> (EntityId, Vec<EdgeId>) {
    let mut current = start.to_string();
    let mut visited: HashSet<String> = HashSet::from([current.clone()]);
    let mut followed = Vec::new();
    loop {
        let next = graph.out_edges(&current).into_iter().find(|(_, e)| {
            e.relation == Relation::ResolvesTo
                && !visited.contains(&e.to)
                && graph.entity(&e.to).is_some()
        });
        match next {
            Some((eid, e)) => {
                followed.push(eid);
                visited.insert(e.to.clone());
                current = e.to.clone();
            }
            None => break,
        }
    }
    (current, followed)
}

/// Dijkstra de `from` até `to` sobre arestas de saída atravessáveis — mais as
/// arestas de entrada de relação simétrica (`Peer`), percorridas no sentido
/// inverso com o mesmo custo e registradas em `edge_in` com o mesmo `EdgeId`.
/// Devolve os passos na ordem origem → destino (`None` se não há caminho ou se
/// `from` não existe no grafo). Arestas para entidades ausentes do grafo são
/// ignoradas.
///
/// Determinismo: nós são assentados por `(custo, ordem de descoberta)` e uma
/// relaxação só substitui a anterior se for estritamente melhor, então, entre
/// caminhos de custo igual, vence o que foi descoberto primeiro — isto é, o
/// que usa as arestas inseridas antes (saída antes de entrada simétrica).
pub fn shortest_path(graph: &Graph, from: &str, to: &str) -> Option<Vec<PathStep>> {
    let from = graph.entity(from)?.id.as_str();
    let mut dist: HashMap<&str, u64> = HashMap::new();
    let mut prev: HashMap<&str, (&str, EdgeId)> = HashMap::new();
    let mut settled: HashSet<&str> = HashSet::new();
    // `seq` é a ordem de descoberta; o nó de cada seq fica em `discovered`.
    let mut discovered: Vec<&str> = Vec::new();
    let mut heap: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();

    dist.insert(from, 0);
    discovered.push(from);
    heap.push(Reverse((0, 0)));

    while let Some(Reverse((cost, seq))) = heap.pop() {
        let Some(&node) = discovered.get(seq) else {
            continue;
        };
        if settled.contains(node) || dist.get(node).is_some_and(|&d| cost > d) {
            continue;
        }
        settled.insert(node);
        if node == to {
            break;
        }
        let outgoing = graph
            .out_edges(node)
            .into_iter()
            .filter(|(_, e)| is_traversable(e.relation))
            .map(|(eid, e)| (eid, e, e.to.as_str()));
        let symmetric_incoming = graph
            .in_edges(node)
            .into_iter()
            .filter(|(_, e)| is_symmetric(e.relation) && is_traversable(e.relation))
            .map(|(eid, e)| (eid, e, e.from.as_str()));
        for (eid, edge, next) in outgoing.chain(symmetric_incoming) {
            if graph.entity(next).is_none() {
                continue;
            }
            let candidate = cost.saturating_add(cost_units(edge));
            let better = dist.get(next).is_none_or(|&d| candidate < d);
            if better {
                dist.insert(next, candidate);
                prev.insert(next, (node, eid));
                let seq = discovered.len();
                discovered.push(next);
                heap.push(Reverse((candidate, seq)));
            }
        }
    }

    if !settled.contains(to) {
        return None;
    }
    let mut steps = Vec::new();
    let mut cursor: &str = to;
    loop {
        match prev.get(cursor) {
            Some(&(p, eid)) => {
                steps.push(PathStep {
                    entity: cursor.to_string(),
                    edge_in: Some(eid),
                });
                cursor = p;
            }
            None => {
                steps.push(PathStep {
                    entity: cursor.to_string(),
                    edge_in: None,
                });
                break;
            }
        }
    }
    steps.reverse();
    Some(steps)
}

/// Branches: para cada hop, vizinhos (saída e entrada) que não estão no
/// caminho, via qualquer relação exceto `Contains`/`Correlated`. Cada entidade
/// aparece uma vez (na primeira âncora em que é vista); `exclude` lista ids
/// que nunca viram branch (ex.: a entidade DNS desembrulhada do destino).
pub fn collect_branches(graph: &Graph, hops: &[Hop], exclude: &[&str]) -> Vec<Branch> {
    let on_path: HashSet<&str> = hops.iter().map(|h| h.entity.as_str()).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for hop in hops {
        for (eid, edge, ent) in graph.neighbors(&hop.entity) {
            if !is_branch_relation(edge.relation)
                || on_path.contains(ent.id.as_str())
                || exclude.contains(&ent.id.as_str())
                || !seen.insert(ent.id.as_str())
            {
                continue;
            }
            let direction = if edge.from == hop.entity {
                Direction::Outgoing
            } else {
                Direction::Incoming
            };
            out.push(Branch {
                anchor: hop.index,
                entity: ent.id.clone(),
                name: ent.name.clone(),
                kind: ent.kind,
                status: ent.status(),
                address: ent
                    .primary_address()
                    .map(|a| a.display())
                    .unwrap_or_default(),
                edge: eid,
                edge_status: edge.status(),
                direction,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use peabiru_core::{Confidence, Entity, EntityKind, Evidence, EvidenceKind, Provider};

    fn ent(id: &str) -> Entity {
        Entity::new(id, id, EntityKind::Host, Provider::Demo)
    }

    fn observed(pct: u8) -> Evidence {
        Evidence::new(
            EvidenceKind::Observed,
            "eBPF",
            Provider::Demo,
            Confidence(pct),
        )
    }

    fn inferred(pct: u8) -> Evidence {
        Evidence::new(
            EvidenceKind::Inferred,
            "route inference",
            Provider::Demo,
            Confidence(pct),
        )
    }

    fn ids(steps: &[PathStep]) -> Vec<&str> {
        steps.iter().map(|s| s.entity.as_str()).collect()
    }

    #[test]
    fn prefers_observed_path_with_same_hop_count() {
        let mut g = Graph::new();
        for id in ["a", "x", "y", "b"] {
            g.upsert_entity(ent(id));
        }
        // a → x → b inferido (inserido primeiro), a → y → b observado.
        g.add_edge(Edge::new("a", "x", Relation::Connects).with_evidence(inferred(60)));
        g.add_edge(Edge::new("x", "b", Relation::Connects).with_evidence(inferred(60)));
        g.add_edge(Edge::new("a", "y", Relation::Connects).with_evidence(observed(95)));
        g.add_edge(Edge::new("y", "b", Relation::Connects).with_evidence(observed(95)));
        let steps = shortest_path(&g, "a", "b").unwrap();
        assert_eq!(ids(&steps), vec!["a", "y", "b"]);
        assert_eq!(steps[0].edge_in, None);
        assert_eq!(steps[1].edge_in, Some(EdgeId(2)));
        assert_eq!(steps[2].edge_in, Some(EdgeId(3)));
    }

    #[test]
    fn fewer_hops_beat_confidence_and_ties_follow_insertion_order() {
        let mut g = Graph::new();
        for id in ["a", "m", "n", "b"] {
            g.upsert_entity(ent(id));
        }
        // Dois caminhos de custo idêntico: a→m→b e a→n→b; o primeiro inserido vence.
        g.add_edge(Edge::new("a", "n", Relation::Connects).with_evidence(observed(90)));
        g.add_edge(Edge::new("n", "b", Relation::Connects).with_evidence(observed(90)));
        g.add_edge(Edge::new("a", "m", Relation::Connects).with_evidence(observed(90)));
        g.add_edge(Edge::new("m", "b", Relation::Connects).with_evidence(observed(90)));
        assert_eq!(
            ids(&shortest_path(&g, "a", "b").unwrap()),
            vec!["a", "n", "b"]
        );

        // Uma aresta direta sem evidência (custo 1.5) ganha de dois hops (>= 2.0).
        g.add_edge(Edge::new("a", "b", Relation::RoutesTo));
        assert_eq!(ids(&shortest_path(&g, "a", "b").unwrap()), vec!["a", "b"]);
    }

    #[test]
    fn ignores_contains_correlated_and_incoming_edges() {
        let mut g = Graph::new();
        for id in ["a", "b", "c"] {
            g.upsert_entity(ent(id));
        }
        g.add_edge(Edge::new("a", "b", Relation::Contains));
        g.add_edge(Edge::new("a", "b", Relation::Correlated));
        g.add_edge(Edge::new("b", "a", Relation::Connects));
        assert!(shortest_path(&g, "a", "b").is_none());
        assert_eq!(ids(&shortest_path(&g, "b", "a").unwrap()), vec!["b", "a"]);
        assert_eq!(ids(&shortest_path(&g, "a", "a").unwrap()), vec!["a"]);
        assert!(shortest_path(&g, "zz", "a").is_none());
        assert!(shortest_path(&g, "a", "c").is_none());
        // aresta para entidade inexistente é ignorada
        g.add_edge(Edge::new("a", "ghost", Relation::Connects));
        assert!(shortest_path(&g, "a", "ghost").is_none());
    }

    #[test]
    fn peer_edges_are_traversed_in_both_directions() {
        let mut g = Graph::new();
        for id in ["a", "b", "c"] {
            g.upsert_entity(ent(id));
        }
        // a ↔ b (Peer, guardada como a → b); b → c (Connects, dirigida).
        g.add_edge(Edge::new("a", "b", Relation::Peer).with_evidence(observed(90)));
        g.add_edge(Edge::new("b", "c", Relation::Connects).with_evidence(observed(90)));

        let fwd = shortest_path(&g, "a", "b").unwrap();
        assert_eq!(ids(&fwd), vec!["a", "b"]);
        assert_eq!(fwd[1].edge_in, Some(EdgeId(0)));

        let rev = shortest_path(&g, "b", "a").unwrap();
        assert_eq!(ids(&rev), vec!["b", "a"]);
        assert_eq!(
            rev[1].edge_in,
            Some(EdgeId(0)),
            "same edge, reverse direction"
        );

        // A simetria é só da Peer: c não volta para b pela Connects.
        assert_eq!(
            ids(&shortest_path(&g, "a", "c").unwrap()),
            vec!["a", "b", "c"]
        );
        assert!(shortest_path(&g, "c", "a").is_none());
        assert!(is_symmetric(Relation::Peer));
        assert!(!is_symmetric(Relation::Correlated));
        assert!(!is_symmetric(Relation::Connects));

        // Custo igual nos dois sentidos: a Peer inversa (custo 1.1) ganha de
        // uma Connects sem evidência (custo 1.5) no mesmo par de nós.
        g.add_edge(Edge::new("b", "a", Relation::Connects));
        let rev = shortest_path(&g, "b", "a").unwrap();
        assert_eq!(rev[1].edge_in, Some(EdgeId(0)));
    }

    #[test]
    fn unwraps_resolves_to_chain_with_cycle_guard() {
        let mut g = Graph::new();
        for id in ["dns://a", "dns://b", "svc"] {
            g.upsert_entity(ent(id));
        }
        g.add_edge(Edge::new("dns://a", "dns://b", Relation::ResolvesTo));
        g.add_edge(Edge::new("dns://b", "svc", Relation::ResolvesTo));
        g.add_edge(Edge::new("svc", "dns://a", Relation::ResolvesTo));
        let t = resolve_target(&g, "dns://a").unwrap();
        assert_eq!(t.selected, "dns://a");
        assert_eq!(t.entity, "svc");
        assert_eq!(t.resolution, vec![EdgeId(0), EdgeId(1)]);
        assert!(t.was_unwrapped());
        // Os `from` das arestas seguidas são os aliases da cadeia (selected
        // primeiro): é o que o engine exclui das branches.
        let aliases: Vec<&str> = t
            .resolution
            .iter()
            .filter_map(|eid| g.edge(*eid))
            .map(|e| e.from.as_str())
            .collect();
        assert_eq!(aliases, vec!["dns://a", "dns://b"]);

        let plain = resolve_target(&g, "svc").unwrap();
        // svc → dns://a → dns://b → (svc já visitado): termina em dns://b
        assert_eq!(plain.entity, "dns://b");
        assert!(resolve_target(&g, "nope").is_none());

        // ResolvesTo para entidade ausente não desembrulha
        let mut g2 = Graph::new();
        g2.upsert_entity(ent("dns://x"));
        g2.add_edge(Edge::new("dns://x", "missing", Relation::ResolvesTo));
        let t2 = resolve_target(&g2, "dns://x").unwrap();
        assert_eq!(t2.entity, "dns://x");
        assert!(!t2.was_unwrapped());
    }
}
