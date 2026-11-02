//! Helpers compartilhados por todos os painéis: blocos com borda, linhas
//! chave/valor, barra de confiança, glifos de status, spinner, formatação.

use chrono::Local;
use peabiru_core::{EvidenceKind, NodeStatus, Timestamp};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};
use ratatui::Frame;

use crate::theme::Theme;

/// Largura da coluna de chave em `kv_line` (`"CORRELATIONS"` + 1 espaço).
pub const KEY_WIDTH: usize = 13;

/// Quadros do spinner (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`), um por tick.
pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Bloco padrão de painel: `Borders::ALL`, `BorderType::Rounded`, título em
/// MAIÚSCULAS na cor `title`, borda `border`/`border_focus`, fundo `panel_bg`
/// (só quando `theme.paint_bg`).
pub fn panel_block<'a>(theme: &Theme, title: &str, focused: bool) -> Block<'a> {
    let mut block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border(focused))
        .title(Line::from(Span::styled(
            format!(" {} ", title.to_uppercase()),
            theme.title(),
        )));
    if theme.paint_bg {
        block = block.style(Style::default().bg(theme.panel_bg));
    }
    block
}

/// Como `panel_block`, mas com o título já montado (sem passar para
/// maiúsculas): sufixos como ` · tracing… ⠋` ou o nome do nó no popup.
pub fn panel_block_line<'a>(theme: &Theme, title: Line<'a>, focused: bool) -> Block<'a> {
    let mut spans = vec![Span::styled(" ", theme.title())];
    spans.extend(title.spans);
    spans.push(Span::styled(" ", theme.title()));
    let mut block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border(focused))
        .title(Line::from(spans));
    if theme.paint_bg {
        block = block.style(Style::default().bg(theme.panel_bg));
    }
    block
}

/// `panel_block` + um título extra alinhado à direita (ex.: `[p] pause`).
pub fn panel_block_with_right<'a>(
    theme: &Theme,
    title: &str,
    right: Line<'a>,
    focused: bool,
) -> Block<'a> {
    panel_block(theme, title, focused).title_top(right.right_aligned())
}

/// `KEY          value`: chave em `muted` preenchida até `KEY_WIDTH`.
pub fn kv_line<'a>(theme: &Theme, key: &str, value: Span<'a>) -> Line<'a> {
    kv_line_width(theme, key, value, KEY_WIDTH)
}

/// `kv_line` com largura de chave customizada.
pub fn kv_line_width<'a>(theme: &Theme, key: &str, value: Span<'a>, width: usize) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{key:<width$}"), theme.muted()),
        value,
    ])
}

/// Barra de `blocks` quadrados `■`: preenchidos em `observed`, vazios em `border`.
pub fn confidence_bar(theme: &Theme, pct: u8, blocks: usize) -> Line<'static> {
    let pct = u32::from(pct.min(100));
    let filled = ((pct * blocks as u32).div_ceil(100)) as usize;
    let filled = filled.min(blocks);
    Line::from(vec![
        Span::styled("■".repeat(filled), Style::default().fg(theme.observed)),
        Span::styled(
            "■".repeat(blocks - filled),
            Style::default().fg(theme.border),
        ),
    ])
}

/// Glifo do status: Observed `●`, Inferred `○`, Correlated `◇`, Probe `◆`.
pub fn status_glyph(status: NodeStatus) -> &'static str {
    match status {
        NodeStatus::Observed => "●",
        NodeStatus::Inferred => "○",
        NodeStatus::Correlated => "◇",
        NodeStatus::Probe => "◆",
    }
}

/// Glifo de uma evidência: observacional `●`, senão `○`.
pub fn evidence_glyph(kind: EvidenceKind) -> &'static str {
    if kind.is_observational() {
        "●"
    } else {
        "○"
    }
}

/// `text` na cor do status.
pub fn status_span<'a>(theme: &Theme, status: NodeStatus, text: impl Into<String>) -> Span<'a> {
    Span::styled(text.into(), theme.status(status))
}

/// `[CORRELATED]` em negrito na cor do status.
pub fn badge<'a>(theme: &Theme, status: NodeStatus) -> Span<'a> {
    Span::styled(
        format!("[{}]", status.label()),
        theme.status(status).add_modifier(Modifier::BOLD),
    )
}

/// Quadro do spinner para o tick atual.
pub fn spinner(tick: u64) -> &'static str {
    SPINNER[(tick % SPINNER.len() as u64) as usize]
}

/// Pinta `area` com `theme.bg` (só quando `theme.paint_bg`).
pub fn fill_bg(frame: &mut Frame, area: Rect, theme: &Theme) {
    fill_bg_buf(frame.buffer_mut(), area, theme);
}

/// Versão de `fill_bg` sobre um `Buffer` (renderers que desenham à mão).
pub fn fill_bg_buf(buf: &mut Buffer, area: Rect, theme: &Theme) {
    if theme.paint_bg {
        buf.set_style(area, Style::default().bg(theme.bg));
    }
}

/// Retângulo `w`×`h` centralizado em `area` (recortado se não couber).
pub fn centered_rect(w: u16, h: u16, area: Rect) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// `2843` → `"2.843s"`.
pub fn fmt_duration_ms(ms: u64) -> String {
    format!("{}.{:03}s", ms / 1000, ms % 1000)
}

/// `1217` → `"1.217s ago"`; a partir de 1 min `"1m 05s ago"`, de 1 h `"1h 02m ago"`.
/// `HH:MM:SS` de `ts` em hora local ou, com `utc`, em UTC (renders
/// determinísticos: `App::fixed_clock`).
pub fn fmt_clock(ts: Timestamp, utc: bool) -> String {
    if utc {
        ts.format("%H:%M:%S").to_string()
    } else {
        ts.with_timezone(&Local).format("%H:%M:%S").to_string()
    }
}

pub fn fmt_ago(ms: u64) -> String {
    let secs = ms / 1000;
    if secs < 60 {
        format!("{} ago", fmt_duration_ms(ms))
    } else if secs < 3600 {
        format!("{}m {:02}s ago", secs / 60, secs % 60)
    } else {
        format!("{}h {:02}m ago", secs / 3600, (secs % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        assert_eq!(fmt_duration_ms(2843), "2.843s");
        assert_eq!(fmt_duration_ms(0), "0.000s");
        assert_eq!(fmt_duration_ms(12_007), "12.007s");
        assert_eq!(fmt_ago(1217), "1.217s ago");
        assert_eq!(fmt_ago(65_000), "1m 05s ago");
        assert_eq!(fmt_ago(3_720_000), "1h 02m ago");
    }

    #[test]
    fn glyphs_and_spinner() {
        assert_eq!(status_glyph(NodeStatus::Observed), "●");
        assert_eq!(status_glyph(NodeStatus::Inferred), "○");
        assert_eq!(status_glyph(NodeStatus::Correlated), "◇");
        assert_eq!(status_glyph(NodeStatus::Probe), "◆");
        assert_eq!(evidence_glyph(EvidenceKind::Observed), "●");
        assert_eq!(evidence_glyph(EvidenceKind::Discovered), "○");
        assert_eq!(spinner(0), "⠋");
        assert_eq!(spinner(9), "⠏");
        assert_eq!(spinner(10), "⠋");
    }

    #[test]
    fn bar_fill_counts() {
        let t = Theme::default_dark();
        let line = confidence_bar(&t, 92, 12);
        assert_eq!(line.spans[0].content.chars().count(), 12);
        assert_eq!(line.spans[1].content.chars().count(), 0);
        let line = confidence_bar(&t, 50, 12);
        assert_eq!(line.spans[0].content.chars().count(), 6);
        assert_eq!(line.spans[1].content.chars().count(), 6);
        let line = confidence_bar(&t, 0, 12);
        assert_eq!(line.spans[0].content.chars().count(), 0);
        assert_eq!(line.spans[1].content.chars().count(), 12);
    }

    #[test]
    fn centered_rect_clamps() {
        let area = Rect::new(0, 0, 100, 40);
        assert_eq!(centered_rect(60, 20, area), Rect::new(20, 10, 60, 20));
        assert_eq!(centered_rect(200, 80, area), area);
    }

    #[test]
    fn kv_line_pads_key() {
        let t = Theme::default_dark();
        let line = kv_line(&t, "HOPS", Span::raw("5"));
        assert_eq!(line.spans[0].content.len(), KEY_WIDTH);
        assert_eq!(badge(&t, NodeStatus::Correlated).content, "[CORRELATED]");
    }
}
