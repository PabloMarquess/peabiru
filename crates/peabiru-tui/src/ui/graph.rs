//! Painel TRACE GRAPH: renderer do grafo (ARCHITECTURE §6.3).
//!
//! O desenho é planejado numa grade virtual (`Plan`) de largura igual à área
//! interna do painel e altura livre. Primeiro entra o caminho principal: cada
//! hop ocupa a linha do nó (glifo em `cx`, nome em `cx+3`), a(s) linha(s) de
//! endereço, a linha da seta (`│`/`┆`) e a da ponta (`▼`); o último hop não tem
//! seta. Depois entram as branches, como conectores "L" tracejados que saem da
//! linha do nó âncora (`╭╌╌╌` à esquerda, `╌╌╌╮` à direita, logo depois do
//! nome) e descem `╎` até o glifo da branch, duas linhas abaixo (a mesma linha
//! da seta do âncora). Uma grade de ocupação garante que nada se sobreponha:
//! sem espaço num lado a branch cai para o outro, no último hop pode descer em
//! linha reta, e em último caso empilha `+3` linhas ou desloca a coluna.
//!
//! O plano vira um `Buffer` local e a janela visível é copiada para o frame
//! com scroll vertical que mantém o nó selecionado à vista.

use std::collections::{HashMap, HashSet};

use peabiru_core::NodeStatus;
use peabiru_trace::TraceResult;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Panel};
use crate::theme::Theme;
use crate::widgets::{centered_rect, panel_block, panel_block_line, spinner, status_glyph};

/// Nome e endereço ficam em `glifo + NAME_GAP` (§6.3: `cx+3`).
const NAME_GAP: u16 = 3;
/// Linha em branco entre a borda superior e o primeiro hop.
const TOP_PAD: u16 = 1;
/// Linha em branco depois do último elemento.
const BOTTOM_PAD: u16 = 1;
/// Glifo de branch à esquerda em `cx - LEFT_OFFSET` (mínimo `LEFT_MIN`).
const LEFT_OFFSET: u16 = 24;
const LEFT_MIN: u16 = 2;
/// Glifo de branch à direita em `cx + RIGHT_OFFSET` (máximo `width - RIGHT_MARGIN`).
const RIGHT_OFFSET: u16 = 26;
const RIGHT_MARGIN: u16 = 20;
/// Branches empilhadas no mesmo lado ficam `STACK_STEP` linhas abaixo.
const STACK_STEP: u16 = 3;
/// Largura mínima útil para o nome/endereço de uma branch.
const MIN_TEXT: u16 = 4;
/// Níveis extras de empilhamento tentados antes de deslocar a coluna.
const MAX_EXTRA_LEVELS: u16 = 6;
/// Deslocamentos da coluna do glifo (em direção a `cx`) tentados em seguida.
const COLUMN_SHIFTS: [u16; 3] = [3, 6, 9];
/// Largura mínima da área interna para desenhar o grafo.
const MIN_GRAPH_WIDTH: u16 = 8;

/// Desenha o painel TRACE GRAPH em `area`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let focused = app.focus == Panel::Graph;
    let block = if app.run.is_running() {
        let title = Line::from(vec![
            Span::styled("TRACE GRAPH", theme.title()),
            Span::styled(format!(" · tracing… {}", spinner(app.tick)), theme.key()),
        ]);
        panel_block_line(theme, title, focused)
    } else {
        panel_block(theme, "TRACE GRAPH", focused)
    };
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(trace) = &app.trace else {
        render_hint(frame, inner, theme);
        return;
    };
    if inner.width < MIN_GRAPH_WIDTH || inner.height == 0 {
        return;
    }

    let plan = build_plan(app, trace, inner.width);
    let selected_row = plan
        .node_rows
        .get(app.selected_node)
        .copied()
        .unwrap_or(TOP_PAD);
    let content = plan.into_buffer(theme);
    let content_h = content.area.height;
    let view_h = inner.height;
    let scroll = if content_h <= view_h {
        0
    } else {
        selected_row
            .saturating_sub(view_h.saturating_sub(2) / 2)
            .min(content_h - view_h)
    };
    blit(frame.buffer_mut(), &content, inner, scroll);

    if scroll > 0 {
        put_right(frame.buffer_mut(), inner, inner.y, "↑ more", theme.muted());
    }
    if scroll.saturating_add(view_h) < content_h {
        let y = inner.bottom().saturating_sub(1);
        put_right(frame.buffer_mut(), inner, y, "↓ more", theme.muted());
    }
}

/// Altura (em linhas) do conteúdo do grafo para uma área interna com `width`
/// colunas; 0 sem trace ou sem largura útil. Útil para testes de scroll.
pub fn layout_height(app: &App, width: u16) -> u16 {
    match &app.trace {
        Some(trace) if width >= MIN_GRAPH_WIDTH => build_plan(app, trace, width).height(),
        _ => 0,
    }
}

/// `press r to run a trace`, centralizado.
fn render_hint(frame: &mut Frame, inner: Rect, theme: &Theme) {
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let hint = Line::from(vec![
        Span::styled("press ", theme.muted()),
        Span::styled("r", theme.key()),
        Span::styled(" to run a trace", theme.muted()),
    ]);
    let rect = centered_rect(inner.width, 1, inner);
    frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), rect);
}

// --- plano de desenho ------------------------------------------------------

/// Lado em que uma branch é desenhada em relação ao caminho principal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Side {
    Left,
    Right,
    /// Em linha reta abaixo do âncora (só no último hop, que não tem seta).
    Down,
}

impl Side {
    fn opposite(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
            Side::Down => Side::Down,
        }
    }
}

/// Uma célula a escrever: símbolo (largura `w`) com estilo.
#[derive(Clone, Debug)]
struct Op {
    x: u16,
    y: u16,
    w: u16,
    sym: String,
    style: Style,
}

impl Op {
    fn sym(x: u16, y: u16, sym: &str, style: Style) -> Self {
        Self {
            x,
            y,
            w: 1,
            sym: sym.to_string(),
            style,
        }
    }
}

/// Texto de um nó (hop ou branch) pronto para desenhar.
struct NodeText {
    glyph: &'static str,
    name: String,
    addrs: Vec<String>,
    glyph_style: Style,
    name_style: Style,
}

/// Posição de um hop, do ponto de vista das branches ancoradas nele.
#[derive(Clone, Copy)]
struct Anchor {
    /// Linha do nó.
    y0: u16,
    /// Linhas de endereço (≥ 1).
    addr_lines: u16,
    /// Primeira coluna livre depois do nome (início do `╌╌╮` à direita).
    name_end: u16,
}

/// Coluna vertical compartilhada pelas branches de um mesmo (âncora, lado):
/// a primeira desenha o canto e o `╎`; as seguintes só prolongam o `╎`.
struct Bus {
    col: u16,
    /// Linha do último glifo pendurado no bus.
    bottom: u16,
    /// Próximo nível de empilhamento livre.
    next_level: u16,
}

/// Candidato de posição para uma branch.
#[derive(Clone, Copy)]
struct Cand {
    side: Side,
    level: u16,
    shift: u16,
}

/// Resultado de um candidato viável: coluna e linha do glifo mais as ops.
struct Placement {
    col: u16,
    row: u16,
    ops: Vec<Op>,
}

/// Grade virtual de desenho.
struct Plan {
    width: u16,
    cx: u16,
    ops: Vec<Op>,
    occupied: HashSet<(u16, u16)>,
    /// Linha do glifo de cada nó, na ordem de `App::graph_nodes()`.
    node_rows: Vec<u16>,
    max_row: u16,
}

impl Plan {
    fn new(width: u16) -> Self {
        // `cx = width * 0.42`, em aritmética inteira.
        let cx = u16::try_from(u32::from(width) * 42 / 100).unwrap_or(u16::MAX);
        Self {
            width,
            cx,
            ops: Vec::new(),
            occupied: HashSet::new(),
            node_rows: Vec::new(),
            max_row: 0,
        }
    }

    fn height(&self) -> u16 {
        self.max_row.saturating_add(1 + BOTTOM_PAD)
    }

    /// Todas as células de `ops` estão dentro da grade e livres?
    fn all_free(&self, ops: &[Op]) -> bool {
        ops.iter().all(|op| {
            (0..op.w).all(|i| {
                let x = op.x.saturating_add(i);
                x < self.width && !self.occupied.contains(&(x, op.y))
            })
        })
    }

    /// Escreve `ops` na grade, marcando as células como ocupadas.
    fn commit(&mut self, ops: Vec<Op>) {
        for op in ops {
            if op.x >= self.width {
                continue;
            }
            for i in 0..op.w {
                self.occupied.insert((op.x.saturating_add(i), op.y));
            }
            self.max_row = self.max_row.max(op.y);
            self.ops.push(op);
        }
    }

    /// Glifo em `(col, row)`, nome e endereços em `col + NAME_GAP`, cada texto
    /// recortado em `avail` colunas. Devolve as ops e a primeira coluna livre
    /// depois do nome.
    fn node_ops(
        &self,
        col: u16,
        row: u16,
        node: &NodeText,
        avail: u16,
        addr_style: Style,
    ) -> (Vec<Op>, u16) {
        let mut ops = vec![Op::sym(col, row, node.glyph, node.glyph_style)];
        let tx = col.saturating_add(NAME_GAP);
        let name_end = text_ops(
            tx,
            row,
            &fit(&node.name, avail),
            node.name_style,
            self.width,
            &mut ops,
        );
        for (i, addr) in node.addrs.iter().enumerate() {
            let dy = u16::try_from(i + 1).unwrap_or(u16::MAX);
            text_ops(
                tx,
                row.saturating_add(dy),
                &fit(addr, avail),
                addr_style,
                self.width,
                &mut ops,
            );
        }
        (ops, name_end)
    }

    /// Nó listado abaixo de tudo, sem conector (degradação quando nenhuma
    /// posição está livre, ou sem hops).
    fn fallback_ops(&self, node: &NodeText, muted: Style) -> Placement {
        let row = self.max_row.saturating_add(2);
        let avail = self.width.saturating_sub(LEFT_MIN + NAME_GAP);
        let (ops, _) = self.node_ops(LEFT_MIN, row, node, avail, muted);
        Placement {
            col: LEFT_MIN,
            row,
            ops,
        }
    }

    /// Tenta desenhar uma branch à esquerda/direita do âncora conforme `cand`.
    fn side_ops(
        &self,
        anchor: &Anchor,
        cand: Cand,
        bus: Option<&Bus>,
        node: &NodeText,
        conn: Style,
        muted: Style,
    ) -> Option<Placement> {
        let cx = self.cx;
        let row = anchor
            .y0
            .saturating_add(2)
            .saturating_add(STACK_STEP.saturating_mul(cand.level));
        let (col, avail) = match cand.side {
            Side::Left => {
                let base = LEFT_MIN.max(cx.saturating_sub(LEFT_OFFSET));
                let col = bus.map_or(base.saturating_add(cand.shift), |b| b.col);
                let text_x = col.saturating_add(NAME_GAP);
                // O texto termina antes da coluna do glifo do hop, com 1 de folga.
                let limit = cx.checked_sub(1)?;
                if text_x.saturating_add(MIN_TEXT) > limit {
                    return None;
                }
                (col, limit - text_x)
            }
            Side::Right => {
                let base = cx
                    .saturating_add(RIGHT_OFFSET)
                    .min(self.width.saturating_sub(RIGHT_MARGIN));
                let col = bus.map_or(base.saturating_sub(cand.shift), |b| b.col);
                // Pelo menos um `╌` entre o nome do hop e o canto.
                if col < anchor.name_end.saturating_add(1) {
                    return None;
                }
                let text_x = col.saturating_add(NAME_GAP);
                if text_x.saturating_add(MIN_TEXT) > self.width {
                    return None;
                }
                (col, self.width - text_x)
            }
            Side::Down => return None,
        };

        let mut ops = Vec::new();
        match bus {
            None => {
                let (corner, from, to) = match cand.side {
                    Side::Left => ("╭", col.saturating_add(1), cx.saturating_sub(1)),
                    _ => ("╮", anchor.name_end, col.saturating_sub(1)),
                };
                ops.push(Op::sym(col, anchor.y0, corner, conn));
                for x in from..=to {
                    ops.push(Op::sym(x, anchor.y0, "╌", conn));
                }
                for y in anchor.y0.saturating_add(1)..row {
                    ops.push(Op::sym(col, y, "╎", conn));
                }
            }
            Some(b) => {
                for y in b.bottom.saturating_add(1)..row {
                    ops.push(Op::sym(col, y, "╎", conn));
                }
            }
        }
        let (node_ops, _) = self.node_ops(col, row, node, avail, muted);
        ops.extend(node_ops);
        self.all_free(&ops).then_some(Placement { col, row, ops })
    }

    /// Tenta pendurar a branch em linha reta abaixo do âncora (último hop).
    fn down_ops(
        &self,
        anchor: &Anchor,
        node: &NodeText,
        conn: Style,
        muted: Style,
    ) -> Option<Placement> {
        let cx = self.cx;
        let row = anchor
            .y0
            .saturating_add(3.max(anchor.addr_lines.saturating_add(1)));
        let text_x = cx.saturating_add(NAME_GAP);
        if text_x.saturating_add(MIN_TEXT) > self.width {
            return None;
        }
        let mut ops: Vec<Op> = (anchor.y0.saturating_add(1)..row)
            .map(|y| Op::sym(cx, y, "╎", conn))
            .collect();
        let (node_ops, _) = self.node_ops(cx, row, node, self.width - text_x, muted);
        ops.extend(node_ops);
        self.all_free(&ops)
            .then_some(Placement { col: cx, row, ops })
    }

    /// Materializa a grade num `Buffer` (origem `(0, 0)`).
    fn into_buffer(self, theme: &Theme) -> Buffer {
        let area = Rect::new(0, 0, self.width, self.height());
        let mut buf = Buffer::empty(area);
        if theme.paint_bg {
            buf.set_style(area, Style::default().bg(theme.panel_bg));
        }
        for op in &self.ops {
            put_sym(&mut buf, op.x, op.y, &op.sym, op.w, op.style);
        }
        buf
    }
}

/// Monta o plano completo: hops, conectores do caminho e branches.
fn build_plan(app: &App, trace: &TraceResult, width: u16) -> Plan {
    let theme = &app.theme;
    let mut plan = Plan::new(width);
    let cx = plan.cx;
    let muted = theme.muted();
    let selected = app.selected_node;

    // --- caminho principal ---------------------------------------------
    let mut anchors: Vec<Anchor> = Vec::with_capacity(trace.hops.len());
    let mut y = TOP_PAD;
    let hop_avail = width.saturating_sub(cx.saturating_add(NAME_GAP));
    for (i, hop) in trace.hops.iter().enumerate() {
        let node = node_text(
            app,
            &hop.entity,
            &hop.name,
            &hop.address,
            hop.status,
            selected == i,
        );
        let (ops, name_end) = plan.node_ops(cx, y, &node, hop_avail, muted);
        plan.commit(ops);
        plan.node_rows.push(y);
        let addr_lines = u16::try_from(node.addrs.len()).unwrap_or(u16::MAX).max(1);
        anchors.push(Anchor {
            y0: y,
            addr_lines,
            name_end,
        });
        let after = y.saturating_add(1).saturating_add(addr_lines);
        match trace.hops.get(i + 1) {
            Some(next) => {
                let (bar, style) = if next.edge_status.is_observed() {
                    ("│", Style::default().fg(theme.observed))
                } else {
                    ("┆", muted)
                };
                plan.commit(vec![
                    Op::sym(cx, after, bar, style),
                    Op::sym(cx, after.saturating_add(1), "▼", style),
                ]);
                y = after.saturating_add(2);
            }
            None => plan.max_row = plan.max_row.max(after.saturating_sub(1)),
        }
    }

    // --- branches --------------------------------------------------------
    let first_side = if cx > width / 2 {
        Side::Left
    } else {
        Side::Right
    };
    let last_hop = trace.hops.len().saturating_sub(1);
    let mut buses: HashMap<(usize, Side), Bus> = HashMap::new();
    let mut last_side: HashMap<usize, Side> = HashMap::new();
    let mut down_used: HashSet<usize> = HashSet::new();

    for (bi, branch) in trace.branches.iter().enumerate() {
        let anchor_idx = branch.anchor.min(last_hop);
        let is_selected = selected == trace.hops.len() + bi;
        let node = node_text(
            app,
            &branch.entity,
            &branch.name,
            &branch.address,
            branch.status,
            is_selected,
        );
        let conn = if branch.status == NodeStatus::Correlated {
            Style::default().fg(theme.correlated)
        } else {
            muted
        };
        let Some(anchor) = anchors.get(anchor_idx).copied() else {
            let p = plan.fallback_ops(&node, muted);
            plan.node_rows.push(p.row);
            plan.commit(p.ops);
            continue;
        };

        let preferred = match last_side.get(&anchor_idx) {
            Some(Side::Left) => Side::Right,
            Some(Side::Right) => Side::Left,
            _ => first_side,
        };
        let order = [preferred, preferred.opposite()];
        let has_bus = |side: Side| buses.contains_key(&(anchor_idx, side));
        let allow_down = anchor_idx == last_hop && !down_used.contains(&anchor_idx);

        // Ordem de preferência: lado ainda vazio; em linha reta (último hop);
        // empilhar num bus existente; por fim, deslocar a coluna de um lado vazio.
        let mut cands: Vec<Cand> = Vec::new();
        for side in order {
            if !has_bus(side) {
                cands.push(Cand {
                    side,
                    level: 0,
                    shift: 0,
                });
            }
        }
        if allow_down {
            cands.push(Cand {
                side: Side::Down,
                level: 0,
                shift: 0,
            });
        }
        for extra in 0..=MAX_EXTRA_LEVELS {
            for side in order {
                let base = buses.get(&(anchor_idx, side)).map_or(0, |b| b.next_level);
                let level = base.saturating_add(extra);
                if level > 0 {
                    cands.push(Cand {
                        side,
                        level,
                        shift: 0,
                    });
                }
            }
        }
        for shift in COLUMN_SHIFTS {
            for extra in 0..=MAX_EXTRA_LEVELS {
                for side in order {
                    if !has_bus(side) {
                        cands.push(Cand {
                            side,
                            level: extra,
                            shift,
                        });
                    }
                }
            }
        }

        let mut placed: Option<(Cand, Placement)> = None;
        for cand in cands {
            let placement = match cand.side {
                Side::Down => plan.down_ops(&anchor, &node, conn, muted),
                _ => {
                    let bus = buses.get(&(anchor_idx, cand.side));
                    plan.side_ops(&anchor, cand, bus, &node, conn, muted)
                }
            };
            if let Some(p) = placement {
                placed = Some((cand, p));
                break;
            }
        }

        match placed {
            Some((cand, p)) => {
                plan.node_rows.push(p.row);
                last_side.insert(anchor_idx, cand.side);
                match cand.side {
                    Side::Down => {
                        down_used.insert(anchor_idx);
                    }
                    side => {
                        buses
                            .entry((anchor_idx, side))
                            .and_modify(|b| {
                                b.bottom = p.row;
                                b.next_level = cand.level.saturating_add(1);
                            })
                            .or_insert(Bus {
                                col: p.col,
                                bottom: p.row,
                                next_level: cand.level.saturating_add(1),
                            });
                    }
                }
                plan.commit(p.ops);
            }
            None => {
                let p = plan.fallback_ops(&node, muted);
                plan.node_rows.push(p.row);
                plan.commit(p.ops);
            }
        }
    }

    plan
}

/// Glifo, nome e endereços de um nó com os estilos do tema. Nome em `text`
/// (`muted` se Inferred); selecionado: glifo `REVERSED`, nome `BOLD`.
fn node_text(
    app: &App,
    entity: &str,
    name: &str,
    fallback_addr: &str,
    status: NodeStatus,
    selected: bool,
) -> NodeText {
    let theme = &app.theme;
    let mut glyph_style = theme.status(status);
    let mut name_style = if status == NodeStatus::Inferred {
        theme.muted()
    } else {
        theme.text()
    };
    if selected {
        glyph_style = glyph_style.add_modifier(Modifier::REVERSED);
        name_style = name_style.add_modifier(Modifier::BOLD);
    }
    NodeText {
        glyph: status_glyph(status),
        name: name.to_string(),
        addrs: addresses_of(app, entity, fallback_addr),
        glyph_style,
        name_style,
    }
}

/// Todos os endereços da entidade (sem repetir; a principal primeiro), ou o
/// endereço que o trace já formatou; sempre ≥ 1 entrada (vazia se nenhum),
/// para manter o ritmo de 4 linhas por hop.
fn addresses_of(app: &App, entity: &str, fallback: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(e) = app.graph.entity(entity) {
        for addr in &e.addresses {
            let s = addr.display();
            if !s.is_empty() && !out.contains(&s) {
                out.push(s);
            }
        }
    }
    if out.is_empty() && !fallback.is_empty() {
        out.push(fallback.to_string());
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Recorta `s` em `max` colunas de exibição, com `…` no fim se não couber.
fn fit(s: &str, max: u16) -> String {
    let max = usize::from(max);
    if s.width() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > max - 1 {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

/// Gera uma op por caractere de `s` a partir de `(x, y)`, parando na borda
/// `width`. Devolve a primeira coluna livre depois do texto.
fn text_ops(x: u16, y: u16, s: &str, style: Style, width: u16, ops: &mut Vec<Op>) -> u16 {
    let mut x = x;
    for c in s.chars() {
        let Some(w) = c.width().filter(|w| *w > 0) else {
            continue;
        };
        let w = u16::try_from(w).unwrap_or(1);
        if x.saturating_add(w) > width {
            break;
        }
        ops.push(Op {
            x,
            y,
            w,
            sym: c.to_string(),
            style,
        });
        x = x.saturating_add(w);
    }
    x
}

// --- escrita segura no buffer ----------------------------------------------

/// Escreve um símbolo de largura `w` em `(x, y)`; fora do buffer é ignorado.
fn put_sym(buf: &mut Buffer, x: u16, y: u16, sym: &str, w: u16, style: Style) {
    let Some(cell) = buf.cell_mut((x, y)) else {
        return;
    };
    cell.set_symbol(sym);
    cell.set_style(style);
    for i in 1..w {
        if let Some(next) = buf.cell_mut((x.saturating_add(i), y)) {
            next.reset();
        }
    }
}

/// Escreve `s` alinhado à direita de `area` na linha `y` (só se couber).
fn put_right(buf: &mut Buffer, area: Rect, y: u16, s: &str, style: Style) {
    let w = u16::try_from(s.width()).unwrap_or(u16::MAX);
    if w == 0 || w > area.width || y < area.y || y >= area.bottom() {
        return;
    }
    let mut ops = Vec::new();
    text_ops(area.right() - w, y, s, style, area.right(), &mut ops);
    for op in ops {
        put_sym(buf, op.x, op.y, &op.sym, op.w, op.style);
    }
}

/// Copia a janela `[scroll, scroll + inner.height)` de `src` para `dst` em `inner`.
fn blit(dst: &mut Buffer, src: &Buffer, inner: Rect, scroll: u16) {
    for y in 0..inner.height {
        let sy = scroll.saturating_add(y);
        for x in 0..inner.width {
            let Some(cell) = src.cell((x, sy)) else {
                continue;
            };
            let (fx, fy) = (inner.x.saturating_add(x), inner.y.saturating_add(y));
            if let Some(target) = dst.cell_mut((fx, fy)) {
                *target = cell.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peabiru_discovery::demo;

    fn traced_app() -> App {
        let mut app = App::new(
            demo::graph(),
            demo::default_request(),
            demo::environment(),
            Theme::default_dark(),
        );
        app.run_trace();
        app
    }

    fn rows_text(buf: &Buffer) -> Vec<String> {
        crate::snapshot::buffer_to_string(buf, false)
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn fit_truncates_with_ellipsis() {
        assert_eq!(fit("abcdef", 10), "abcdef");
        assert_eq!(fit("abcdef", 6), "abcdef");
        assert_eq!(fit("abcdef", 4), "abc…");
        assert_eq!(fit("abcdef", 1), "…");
        assert_eq!(fit("abcdef", 0), "");
    }

    #[test]
    fn plan_places_every_node_and_tracks_rows() {
        let app = traced_app();
        let trace = app.trace.as_ref().unwrap();
        let plan = build_plan(&app, trace, 69);
        assert_eq!(plan.node_rows.len(), app.graph_nodes().len());
        // Hops de cima para baixo, 4 linhas cada (aws-alb tem 2 endereços: 5).
        assert_eq!(&plan.node_rows[..5], &[1, 5, 9, 13, 18]);
        // nat-gw pendurada duas linhas abaixo do istio.
        assert_eq!(plan.node_rows[5], 9 + 2);
        assert_eq!(plan.cx, 28);
        assert_eq!(layout_height(&app, 69), plan.height());
        assert!(plan.height() >= 20);
    }

    #[test]
    fn buffer_matches_mockup_structure() {
        let app = traced_app();
        let trace = app.trace.as_ref().unwrap();
        let buf = build_plan(&app, trace, 69).into_buffer(&app.theme);
        let rows = rows_text(&buf);
        let find = |needle: &str| rows.iter().position(|r| r.contains(needle)).unwrap();
        assert!(rows[find("web-frontend-7f9c8c")].starts_with(&format!("{}● ", " ".repeat(28))));
        assert!(rows[find("aws-alb")].contains("◇  aws-alb"));
        assert!(rows[find("alb2c3.elb.amazonaws.com") + 1].contains("52.95.23.10:443"));
        let istio = find("istio-ingressgateway");
        assert!(rows[istio].contains("╭╌╌"), "{}", rows[istio]);
        assert!(rows[istio + 1].contains('╎'));
        assert!(rows[istio + 2].contains("○  nat-gw"));
        // Último hop: uma branch de cada lado e uma em linha reta.
        let app_svc = find("app-svc");
        assert!(rows[app_svc].contains("app-svc╌╌"), "{}", rows[app_svc]);
        assert!(rows[app_svc + 2].contains("orders-db") || rows[app_svc + 2].contains("redis"));
        assert!(rows.iter().any(|r| r.contains("payments-svc")));
        assert!(rows.iter().all(|r| r.width() <= 69));
    }

    #[test]
    fn selected_branch_is_highlighted_and_connector_colored() {
        let mut app = traced_app();
        let idx = app
            .graph_nodes()
            .iter()
            .position(|n| n.name == "orders-db")
            .unwrap();
        app.selected_node = idx;
        let trace = app.trace.as_ref().unwrap();
        let plan = build_plan(&app, trace, 69);
        let row = plan.node_rows[idx];
        let buf = plan.into_buffer(&app.theme);
        let rows = rows_text(&buf);
        let byte = rows[usize::from(row)].find("orders-db").unwrap();
        let x = u16::try_from(rows[usize::from(row)][..byte].chars().count()).unwrap();
        let name = &buf[(x, row)];
        assert!(name.modifier.contains(Modifier::BOLD));
        let glyph = &buf[(x - NAME_GAP, row)];
        assert_eq!(glyph.symbol(), "◇");
        assert!(glyph.modifier.contains(Modifier::REVERSED));
        assert_eq!(glyph.fg, app.theme.correlated);
        // O conector da branch correlacionada é laranja.
        let conn = &buf[(x - NAME_GAP, row - 1)];
        assert_eq!(conn.symbol(), "╎");
        assert_eq!(conn.fg, app.theme.correlated);
    }

    #[test]
    fn tiny_widths_do_not_panic() {
        let app = traced_app();
        for w in 0..40u16 {
            let _ = layout_height(&app, w);
        }
        let empty = App::new(
            demo::graph(),
            demo::default_request(),
            demo::environment(),
            Theme::default_dark(),
        );
        assert_eq!(layout_height(&empty, 80), 0);
    }
}
