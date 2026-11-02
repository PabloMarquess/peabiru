//! Estado da interface (ARCHITECTURE §6.1): tabs, foco, trace atual, replay
//! de eventos do sensor, seleções e flags.

use std::rc::Rc;
use std::time::Instant;

use chrono::Utc;
use crossterm::event::KeyEvent;
use peabiru_core::{Entity, EntityId, Event, Finding, Graph, NodeStatus, Timestamp};
use peabiru_discovery::demo::Environment;
use peabiru_discovery::Sensor;
use peabiru_trace::{TraceEngine, TraceRequest, TraceResult, TraceStatus};

use crate::input;
use crate::theme::Theme;

/// Tamanho máximo do buffer de eventos (FIFO).
pub const MAX_EVENTS: usize = 500;
/// Duração de um tick em ms (`run.rs` chama `on_tick` nesse ritmo).
pub const TICK_MS: u64 = 100;
/// Em yolo mode, ticks entre o fim de um trace e o próximo (3 s).
pub const YOLO_RERUN_TICKS: u64 = 3000 / TICK_MS;
/// Duração mínima de um run (spinner visível mesmo sem janela de observação).
pub const RUN_MIN_MS: u64 = 1000;

/// Tab ativa (`1`–`5`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    Trace,
    Topology,
    Correlations,
    Observability,
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Trace,
        Tab::Topology,
        Tab::Correlations,
        Tab::Observability,
        Tab::Settings,
    ];

    /// Número da tab como no header (`1`..`5`).
    pub fn number(&self) -> u8 {
        match self {
            Tab::Trace => 1,
            Tab::Topology => 2,
            Tab::Correlations => 3,
            Tab::Observability => 4,
            Tab::Settings => 5,
        }
    }

    /// `"Trace"`, `"Topology"`, ...
    pub fn label(&self) -> &'static str {
        match self {
            Tab::Trace => "Trace",
            Tab::Topology => "Topology",
            Tab::Correlations => "Correlations",
            Tab::Observability => "Observability",
            Tab::Settings => "Settings",
        }
    }

    /// `'1'`..`'5'` → tab.
    pub fn from_digit(c: char) -> Option<Tab> {
        match c {
            '1' => Some(Tab::Trace),
            '2' => Some(Tab::Topology),
            '3' => Some(Tab::Correlations),
            '4' => Some(Tab::Observability),
            '5' => Some(Tab::Settings),
            _ => None,
        }
    }
}

/// Painel focado no Tab Trace (ordem de `Tab`/`Shift-Tab`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Panel {
    TraceTarget,
    Graph,
    NodeDetails,
    Correlations,
    Events,
}

impl Panel {
    pub const ORDER: [Panel; 5] = [
        Panel::TraceTarget,
        Panel::Graph,
        Panel::NodeDetails,
        Panel::Correlations,
        Panel::Events,
    ];

    pub fn next(self) -> Panel {
        let i = Self::ORDER.iter().position(|p| *p == self).unwrap_or(0);
        Self::ORDER[(i + 1) % Self::ORDER.len()]
    }

    pub fn prev(self) -> Panel {
        let i = Self::ORDER.iter().position(|p| *p == self).unwrap_or(0);
        Self::ORDER[(i + Self::ORDER.len() - 1) % Self::ORDER.len()]
    }

    /// Título do painel correspondente.
    pub fn title(&self) -> &'static str {
        match self {
            Panel::TraceTarget => "TRACE TARGET",
            Panel::Graph => "TRACE GRAPH",
            Panel::NodeDetails => "NODE DETAILS",
            Panel::Correlations => "CORRELATIONS",
            Panel::Events => "EVENTS",
        }
    }
}

/// Estado do run atual.
///
/// `Running.next_event` é o cursor de replay sobre a timeline de eventos já
/// conhecidos do sensor (ver `App::on_tick`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunState {
    Idle,
    Running { started: Instant, next_event: usize },
    Done,
}

impl RunState {
    pub fn is_running(&self) -> bool {
        matches!(self, RunState::Running { .. })
    }
}

/// Nó desenhado no TRACE GRAPH (hops e depois branches).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphNode {
    pub entity: EntityId,
    pub name: String,
    pub status: NodeStatus,
    pub is_branch: bool,
}

/// Avaliador de findings chamado ao fim de `run_trace()`: o CLI liga aqui o
/// `RuleEngine` da `peabiru-lua` (a TUI não depende dela). `Rc` para `App`
/// continuar `Clone`.
#[derive(Clone)]
pub struct FindingsFn(pub Rc<FindingsEval>);

/// Assinatura do avaliador de `FindingsFn`.
pub type FindingsEval = dyn Fn(&TraceResult, &Graph) -> Vec<Finding>;

impl std::fmt::Debug for FindingsFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FindingsFn(..)")
    }
}

/// Estado completo da interface.
#[derive(Clone, Debug)]
pub struct App {
    pub tab: Tab,
    pub focus: Panel,
    pub graph: Graph,
    pub request: TraceRequest,
    pub trace: Option<TraceResult>,
    pub run: RunState,
    /// Eventos já emitidos (máx. `MAX_EVENTS`, FIFO).
    pub events: Vec<Event>,
    pub events_paused: bool,
    /// Índice em `graph_nodes()` (Tab Trace) ou em `graph.entities()` (Tab Topology).
    pub selected_node: usize,
    pub selected_correlation: usize,
    pub selected_event: usize,
    /// Auto-run contínuo: ao terminar, reinicia em 3 s.
    pub yolo: bool,
    pub help_open: bool,
    pub details_open: bool,
    pub environment: Environment,
    pub theme: Theme,
    pub findings: Vec<Finding>,
    /// Avalia regras ao fim de `run_trace()` e preenche `findings` (`None`: nada).
    pub findings_fn: Option<FindingsFn>,
    /// Incrementa a cada 100 ms.
    pub tick: u64,
    pub should_quit: bool,
    /// Erro do último `run_trace()` (STATUS `Failed` no TRACE SUMMARY).
    pub last_error: Option<String>,
    /// Relógio fixo para renders determinísticos (snapshots): é o "agora"
    /// de `LAST SEEN`/replay e faz os painéis exibirem horas em UTC.
    /// `None` (padrão): `Utc::now()` e hora local.
    pub fixed_clock: Option<Timestamp>,
    /// Tudo que o sensor já emitiu (máx. `MAX_EVENTS`), base do replay.
    timeline: Vec<Event>,
    /// Quantos eventos da `timeline` pertencem ao replay do run atual.
    replay_len: usize,
    /// Eventos recebidos enquanto pausado (liberados no resume).
    pending: Vec<Event>,
    /// Tick em que o run atual terminou (para o rerun em yolo mode).
    done_tick: Option<u64>,
}

impl App {
    pub fn new(
        graph: Graph,
        request: TraceRequest,
        environment: Environment,
        theme: Theme,
    ) -> Self {
        Self {
            tab: Tab::Trace,
            focus: Panel::Graph,
            graph,
            request,
            trace: None,
            run: RunState::Idle,
            events: Vec::new(),
            events_paused: false,
            selected_node: 0,
            selected_correlation: 0,
            selected_event: 0,
            yolo: false,
            help_open: false,
            details_open: false,
            environment,
            theme,
            findings: Vec::new(),
            findings_fn: None,
            tick: 0,
            should_quit: false,
            last_error: None,
            fixed_clock: None,
            timeline: Vec::new(),
            replay_len: 0,
            pending: Vec::new(),
            done_tick: None,
        }
    }

    // --- trace ---------------------------------------------------------

    /// Executa o `TraceEngine`, zera os eventos exibidos e entra em
    /// `Running` (replay dos eventos já conhecidos do sensor). Em erro,
    /// `trace = None`, `last_error = Some(..)` e o run termina na hora.
    pub fn run_trace(&mut self) {
        match TraceEngine::trace(&self.graph, &self.request) {
            Ok(result) => {
                self.trace = Some(result);
                self.last_error = None;
                self.run = RunState::Running {
                    started: Instant::now(),
                    next_event: 0,
                };
                self.done_tick = None;
            }
            Err(e) => {
                self.trace = None;
                self.last_error = Some(e.to_string());
                self.run = RunState::Done;
                self.done_tick = Some(self.tick);
            }
        }
        self.findings.clear();
        if let (Some(trace), Some(eval)) = (self.trace.as_mut(), self.findings_fn.as_ref()) {
            self.findings = (eval.0)(trace, &self.graph);
            trace.findings = self.findings.clone();
        }
        self.events.clear();
        self.pending.clear();
        self.selected_event = 0;
        self.replay_len = self.timeline.len();
        self.clamp_selection();
    }

    /// "Agora" para os painéis: `fixed_clock` se definido, senão `Utc::now()`.
    pub fn now(&self) -> Timestamp {
        self.fixed_clock.unwrap_or_else(Utc::now)
    }

    /// STATUS do TRACE SUMMARY: `Failed` se `last_error`, senão pelo `run`.
    pub fn trace_status(&self) -> TraceStatus {
        if self.last_error.is_some() {
            return TraceStatus::Failed;
        }
        match self.run {
            RunState::Idle => TraceStatus::Idle,
            RunState::Running { .. } => TraceStatus::Running,
            RunState::Done => TraceStatus::Completed,
        }
    }

    /// Chamado a cada 100 ms pelo loop de `run.rs`:
    ///
    /// 1. `tick += 1`;
    /// 2. faz `poll` no sensor: cada evento novo entra na `timeline` e é
    ///    exibido ao vivo;
    /// 3. se `Running`, re-emite os eventos da timeline anteriores ao run no
    ///    offset original (relativo ao primeiro evento) e termina quando
    ///    `elapsed >= max(duration_ms, RUN_MIN_MS)`;
    /// 4. se `Done` e `yolo`, reinicia após 3 s.
    pub fn on_tick(&mut self, sensor: &mut dyn Sensor) {
        self.tick += 1;
        for ev in sensor.poll() {
            self.push_timeline(ev.clone());
            self.push_event(ev);
        }
        match self.run {
            RunState::Running {
                started,
                next_event,
            } => {
                let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                let mut idx = next_event;
                while idx < self.replay_len.min(self.timeline.len()) {
                    if self.timeline_offset_ms(idx) > elapsed {
                        break;
                    }
                    let mut ev = self.timeline[idx].clone();
                    ev.ts = self.now();
                    self.push_event(ev);
                    idx += 1;
                }
                let window = self
                    .trace
                    .as_ref()
                    .map_or(0, |t| t.summary.duration_ms)
                    .max(RUN_MIN_MS);
                if elapsed >= window {
                    self.run = RunState::Done;
                    self.done_tick = Some(self.tick);
                } else {
                    self.run = RunState::Running {
                        started,
                        next_event: idx,
                    };
                }
            }
            RunState::Done => {
                let due = self
                    .done_tick
                    .is_some_and(|t| self.tick.saturating_sub(t) >= YOLO_RERUN_TICKS);
                if self.yolo && due {
                    self.run_trace();
                }
            }
            RunState::Idle => {}
        }
    }

    /// Delegado a `input::handle`.
    pub fn on_key(&mut self, key: KeyEvent) {
        input::handle(self, key);
    }

    /// Nós na ordem de desenho: hops, depois branches. Vazio sem trace.
    pub fn graph_nodes(&self) -> Vec<GraphNode> {
        let Some(trace) = &self.trace else {
            return Vec::new();
        };
        let hops = trace.hops.iter().map(|h| GraphNode {
            entity: h.entity.clone(),
            name: h.name.clone(),
            status: h.status,
            is_branch: false,
        });
        let branches = trace.branches.iter().map(|b| GraphNode {
            entity: b.entity.clone(),
            name: b.name.clone(),
            status: b.status,
            is_branch: true,
        });
        hops.chain(branches).collect()
    }

    /// Entidade selecionada: `graph_nodes()[selected_node]` (Tab Topology:
    /// `selected_node`-ésima entidade do grafo).
    pub fn selected_entity(&self) -> Option<&Entity> {
        match self.tab {
            Tab::Topology => self.graph.entities().nth(self.selected_node),
            _ => {
                let node = self.graph_nodes().into_iter().nth(self.selected_node)?;
                self.graph.entity(&node.entity)
            }
        }
    }

    /// Ms desde o início do run atual (`None` fora de `Running`).
    pub fn run_elapsed_ms(&self) -> Option<u64> {
        match self.run {
            RunState::Running { started, .. } => {
                Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX))
            }
            _ => None,
        }
    }

    // --- navegação -----------------------------------------------------

    pub fn next_panel(&mut self) {
        self.focus = self.focus.next();
    }

    pub fn prev_panel(&mut self) {
        self.focus = self.focus.prev();
    }

    /// Tamanho da lista navegável no contexto atual (tab + painel focado).
    pub fn selection_len(&self) -> usize {
        let correlations = self.trace.as_ref().map_or(0, |t| t.correlations.len());
        match self.tab {
            Tab::Trace => match self.focus {
                Panel::Graph => self.graph_nodes().len(),
                Panel::Correlations => correlations,
                Panel::Events => self.events.len(),
                Panel::TraceTarget | Panel::NodeDetails => 0,
            },
            Tab::Topology => self.graph.entity_count(),
            Tab::Correlations => correlations,
            Tab::Observability | Tab::Settings => 0,
        }
    }

    fn selection_mut(&mut self) -> Option<&mut usize> {
        match self.tab {
            Tab::Trace => match self.focus {
                Panel::Graph => Some(&mut self.selected_node),
                Panel::Correlations => Some(&mut self.selected_correlation),
                Panel::Events => Some(&mut self.selected_event),
                Panel::TraceTarget | Panel::NodeDetails => None,
            },
            Tab::Topology => Some(&mut self.selected_node),
            Tab::Correlations => Some(&mut self.selected_correlation),
            Tab::Observability | Tab::Settings => None,
        }
    }

    /// `↓`/`j`: próximo item do painel focado (com wrap-around).
    pub fn select_next(&mut self) {
        let n = self.selection_len();
        if let Some(cur) = self.selection_mut().filter(|_| n > 0) {
            *cur = (*cur + 1) % n;
        }
    }

    /// `↑`/`k`: item anterior do painel focado (com wrap-around).
    pub fn select_prev(&mut self) {
        let n = self.selection_len();
        if let Some(cur) = self.selection_mut().filter(|_| n > 0) {
            *cur = (*cur + n - 1) % n;
        }
    }

    /// `g`: primeiro item.
    pub fn select_first(&mut self) {
        let n = self.selection_len();
        if let Some(cur) = self.selection_mut().filter(|_| n > 0) {
            *cur = 0;
        }
    }

    /// `G`: último item.
    pub fn select_last(&mut self) {
        let n = self.selection_len();
        if let Some(cur) = self.selection_mut().filter(|_| n > 0) {
            *cur = n - 1;
        }
    }

    /// `Enter`: Graph/Topology → details; Correlations → seleciona o nó `a`
    /// da correlação e foca o Graph (Tab Trace).
    pub fn activate_selection(&mut self) {
        match (self.tab, self.focus) {
            (Tab::Trace, Panel::Graph) | (Tab::Topology, _) => {
                if self.selected_entity().is_some() {
                    self.details_open = true;
                }
            }
            (Tab::Trace, Panel::Correlations) | (Tab::Correlations, _) => {
                self.select_correlation_node();
            }
            _ => {}
        }
    }

    /// Seleciona no grafo o nó `a` da correlação selecionada, foca o Graph
    /// e volta ao Tab Trace. Sem efeito se não houver correlação.
    pub fn select_correlation_node(&mut self) {
        let Some(id) = self
            .trace
            .as_ref()
            .and_then(|t| t.correlations.get(self.selected_correlation))
            .map(|c| c.a.clone())
        else {
            return;
        };
        if let Some(idx) = self.graph_nodes().iter().position(|n| n.entity == id) {
            self.selected_node = idx;
        }
        self.tab = Tab::Trace;
        self.focus = Panel::Graph;
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.clamp_selection();
    }

    pub fn toggle_help(&mut self) {
        self.help_open = !self.help_open;
    }

    pub fn toggle_details(&mut self) {
        self.details_open = !self.details_open;
    }

    /// Alterna yolo mode; ao ligar em `Idle`, dispara o primeiro trace.
    pub fn toggle_yolo(&mut self) {
        self.yolo = !self.yolo;
        if self.yolo && self.run == RunState::Idle {
            self.run_trace();
        }
    }

    /// Pausa/retoma o stream de eventos. Enquanto pausado, eventos novos
    /// ficam pendentes e entram todos de uma vez no resume.
    pub fn toggle_pause(&mut self) {
        self.events_paused = !self.events_paused;
        if !self.events_paused {
            let pending = std::mem::take(&mut self.pending);
            for ev in pending {
                self.push_event(ev);
            }
        }
    }

    // --- internos ------------------------------------------------------

    fn push_event(&mut self, ev: Event) {
        if self.events_paused {
            push_capped(&mut self.pending, ev);
            return;
        }
        if push_capped(&mut self.events, ev) {
            self.selected_event = self.selected_event.saturating_sub(1);
        }
        self.selected_event = self.events.len().saturating_sub(1);
    }

    fn push_timeline(&mut self, ev: Event) {
        if push_capped(&mut self.timeline, ev) {
            self.replay_len = self.replay_len.saturating_sub(1);
            if let RunState::Running {
                started,
                next_event,
            } = self.run
            {
                self.run = RunState::Running {
                    started,
                    next_event: next_event.saturating_sub(1),
                };
            }
        }
    }

    /// Offset em ms do evento `idx` da timeline relativo ao primeiro (≥ 0).
    fn timeline_offset_ms(&self, idx: usize) -> u64 {
        let (Some(first), Some(ev)) = (self.timeline.first(), self.timeline.get(idx)) else {
            return 0;
        };
        u64::try_from((ev.ts - first.ts).num_milliseconds()).unwrap_or(0)
    }

    fn clamp_selection(&mut self) {
        let nodes = match self.tab {
            Tab::Topology => self.graph.entity_count(),
            _ => self.graph_nodes().len(),
        };
        self.selected_node = self.selected_node.min(nodes.saturating_sub(1));
        let correlations = self.trace.as_ref().map_or(0, |t| t.correlations.len());
        self.selected_correlation = self
            .selected_correlation
            .min(correlations.saturating_sub(1));
        self.selected_event = self.selected_event.min(self.events.len().saturating_sub(1));
    }
}

/// Empurra no fim; se cheio, descarta o mais antigo. `true` se descartou.
fn push_capped(buf: &mut Vec<Event>, ev: Event) -> bool {
    let evicted = buf.len() >= MAX_EVENTS;
    if evicted {
        buf.remove(0);
    }
    buf.push(ev);
    evicted
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;
    use peabiru_core::EventSource;
    use peabiru_discovery::demo;

    struct Replay(Vec<Event>);

    impl Sensor for Replay {
        fn name(&self) -> &'static str {
            "test"
        }
        fn poll(&mut self) -> Vec<Event> {
            std::mem::take(&mut self.0)
        }
    }

    fn app() -> App {
        App::new(
            demo::graph(),
            demo::default_request(),
            demo::environment(),
            Theme::default_dark(),
        )
    }

    fn event(offset_ms: i64) -> Event {
        Event {
            ts: Utc::now() + TimeDelta::milliseconds(offset_ms),
            source: EventSource::Ebpf,
            action: "connect".into(),
            from: "a".into(),
            to: "b".into(),
            bidirectional: false,
            detail: format!("{offset_ms}"),
        }
    }

    #[test]
    fn run_trace_populates_and_graph_nodes_order() {
        let mut app = app();
        assert_eq!(app.trace_status(), TraceStatus::Idle);
        app.run_trace();
        assert_eq!(app.trace_status(), TraceStatus::Running);
        let nodes = app.graph_nodes();
        assert_eq!(nodes.len(), 5 + 4);
        assert!(nodes[..5].iter().all(|n| !n.is_branch));
        assert!(nodes[5..].iter().all(|n| n.is_branch));
        assert_eq!(nodes[0].name, "web-frontend-7f9c8c");
        assert_eq!(
            app.selected_entity().map(|e| e.name.as_str()),
            Some("web-frontend-7f9c8c")
        );
    }

    #[test]
    fn failed_trace_sets_last_error() {
        let mut app = app();
        app.request.to = "nope".into();
        app.run_trace();
        assert!(app.trace.is_none());
        assert!(app.last_error.is_some());
        assert_eq!(app.trace_status(), TraceStatus::Failed);
        assert_eq!(app.run, RunState::Done);
    }

    #[test]
    fn tick_ingests_events_and_caps_buffer() {
        let mut app = app();
        let mut sensor = Replay((0..(MAX_EVENTS as i64 + 20)).map(event).collect());
        app.on_tick(&mut sensor);
        assert_eq!(app.tick, 1);
        assert_eq!(app.events.len(), MAX_EVENTS);
        assert_eq!(app.selected_event, MAX_EVENTS - 1);
        assert_eq!(app.events[0].detail, "20");
    }

    #[test]
    fn running_replays_timeline_and_finishes() {
        let mut app = app();
        let mut sensor = Replay(vec![event(0), event(50)]);
        app.on_tick(&mut sensor);
        assert_eq!(app.events.len(), 2);
        app.run_trace();
        assert!(app.events.is_empty());
        std::thread::sleep(std::time::Duration::from_millis(60));
        app.on_tick(&mut sensor);
        assert_eq!(app.events.len(), 2, "replayed from the timeline");
        assert!(app.run.is_running());
        if let RunState::Running { started, .. } = app.run {
            app.run = RunState::Running {
                started: started - std::time::Duration::from_millis(5000),
                next_event: 2,
            };
        }
        app.on_tick(&mut sensor);
        assert_eq!(app.run, RunState::Done);
        assert_eq!(app.trace_status(), TraceStatus::Completed);
    }

    #[test]
    fn yolo_reruns_after_three_seconds() {
        let mut app = app();
        let mut sensor = Replay(Vec::new());
        app.request.to = "nope".into();
        app.run_trace();
        assert_eq!(app.run, RunState::Done);
        app.yolo = true;
        for _ in 0..(YOLO_RERUN_TICKS - 1) {
            app.on_tick(&mut sensor);
        }
        assert_eq!(app.done_tick, Some(0));
        app.on_tick(&mut sensor);
        assert_eq!(app.done_tick, Some(app.tick), "re-ran on the 30th tick");
    }

    #[test]
    fn pause_holds_events_until_resume() {
        let mut app = app();
        app.toggle_pause();
        let mut sensor = Replay(vec![event(0)]);
        app.on_tick(&mut sensor);
        assert!(app.events.is_empty());
        app.toggle_pause();
        assert_eq!(app.events.len(), 1);
    }

    #[test]
    fn navigation_wraps_and_respects_focus() {
        let mut app = app();
        app.run_trace();
        assert_eq!(app.focus, Panel::Graph);
        app.select_prev();
        assert_eq!(app.selected_node, 8);
        app.select_next();
        assert_eq!(app.selected_node, 0);
        app.select_last();
        assert_eq!(app.selected_node, 8);
        app.select_first();
        assert_eq!(app.selected_node, 0);
        app.next_panel();
        assert_eq!(app.focus, Panel::NodeDetails);
        app.select_next();
        assert_eq!(app.selected_node, 0, "NodeDetails has nothing to navigate");
        app.prev_panel();
        app.prev_panel();
        assert_eq!(app.focus, Panel::TraceTarget);
        app.prev_panel();
        assert_eq!(app.focus, Panel::Events);
    }

    #[test]
    fn correlation_enter_selects_node_a() {
        let mut app = app();
        app.run_trace();
        app.focus = Panel::Correlations;
        app.selected_correlation = 1; // web-frontend ↔ orders-db
        app.activate_selection();
        assert_eq!(app.focus, Panel::Graph);
        assert_eq!(app.selected_node, 0);
        app.selected_correlation = 2; // istio ↔ redis
        app.select_correlation_node();
        assert_eq!(
            app.graph_nodes()[app.selected_node].name,
            "istio-ingressgateway"
        );
    }

    #[test]
    fn topology_selection_uses_graph_entities() {
        let mut app = app();
        app.set_tab(Tab::Topology);
        app.select_last();
        assert_eq!(app.selected_node, app.graph.entity_count() - 1);
        assert!(app.selected_entity().is_some());
        app.set_tab(Tab::Trace);
        assert_eq!(app.selected_node, 0, "clamped: no trace yet");
    }
}
