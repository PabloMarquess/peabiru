//! Render para string via `TestBackend` (base dos testes de snapshot e do
//! `peabiru tui --render-once`) e o estado determinístico que ambos
//! renderizam (`prepare`, `fixed_base`, `fixed_clock`).

use chrono::{DateTime, TimeDelta, Utc};
use peabiru_core::{NodeStatus, Timestamp};
use peabiru_discovery::demo::DEMO_DURATION_MS;
use peabiru_discovery::Sensor;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, RunState};
use crate::ui;

/// Base da timeline dos renders determinísticos: 2026-01-01T12:41:02Z, em
/// segundos Unix.
pub const FIXED_BASE_UNIX: i64 = 1_767_271_262;

/// Ticks de `prepare` antes de desistir de esgotar o sensor.
const MAX_PREPARE_TICKS: usize = 100;

/// `2026-01-01T12:41:02Z`: base dos eventos e das evidências do demo nos
/// renders determinísticos (`demo::graph_from`, `DemoSensor::with_base`).
pub fn fixed_base() -> Timestamp {
    DateTime::from_timestamp(FIXED_BASE_UNIX, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}

/// Relógio fixo dos renders determinísticos: fim da janela de observação do
/// demo (`fixed_base()` + `DEMO_DURATION_MS`), o "agora" de `LAST SEEN`.
pub fn fixed_clock() -> Timestamp {
    let window = TimeDelta::milliseconds(i64::try_from(DEMO_DURATION_MS).unwrap_or(i64::MAX));
    fixed_base()
        .checked_add_signed(window)
        .unwrap_or_else(fixed_base)
}

/// Leva `app` ao estado final de um run para um render determinístico, o
/// mesmo de `peabiru tui --render-once` e do teste de snapshot: fixa o
/// relógio em `clock`, `run_trace()`, esgota `sensor` via `on_tick` (todos
/// os eventos entram no EVENTS; `sensor` deve ser de intervalo zero),
/// encerra o run (`Done`, STATUS `Completed`) e seleciona o primeiro hop
/// `Correlated` (o `aws-alb` do mockup) para o NODE DETAILS.
pub fn prepare(app: &mut App, sensor: &mut dyn Sensor, clock: Timestamp) {
    app.fixed_clock = Some(clock);
    app.run_trace();
    for _ in 0..MAX_PREPARE_TICKS {
        let before = app.events.len();
        app.on_tick(sensor);
        if app.events.len() == before {
            break;
        }
    }
    if app.run.is_running() {
        app.run = RunState::Done;
    }
    if let Some(idx) = app
        .graph_nodes()
        .iter()
        .position(|n| !n.is_branch && n.status == NodeStatus::Correlated)
    {
        app.selected_node = idx;
    }
}

/// Desenha `app` num terminal virtual `width`×`height` e devolve as linhas
/// (separadas por `\n`, com `\n` final). `ansi = false`: só os símbolos,
/// espaços finais de cada linha removidos. `ansi = true`: SGR truecolor por
/// mudança de estilo (fg/bg `Rgb`, cores nomeadas/indexadas mapeadas,
/// modificadores) e reset no fim de cada linha.
pub fn render_to_string(app: &App, width: u16, height: u16, ansi: bool) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(e) => return format!("render error: {e}\n"),
    };
    if let Err(e) = terminal.draw(|frame| ui::draw(frame, app)) {
        return format!("render error: {e}\n");
    }
    buffer_to_string(terminal.backend().buffer(), ansi)
}

/// Converte um `Buffer` em texto (ver `render_to_string`). Células de
/// continuação de caracteres largos são puladas.
pub fn buffer_to_string(buf: &Buffer, ansi: bool) -> String {
    let area = buf.area;
    let mut out = String::new();
    for y in area.top()..area.bottom() {
        let mut line = String::new();
        let mut skip = 0usize;
        let mut last_style: Option<(Color, Color, Modifier)> = None;
        for x in area.left()..area.right() {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            let cell = &buf[(x, y)];
            let symbol = cell.symbol();
            skip = symbol.width().saturating_sub(1);
            if ansi {
                let style = (cell.fg, cell.bg, cell.modifier);
                if last_style != Some(style) {
                    line.push_str(&sgr(cell.fg, cell.bg, cell.modifier));
                    last_style = Some(style);
                }
            }
            line.push_str(symbol);
        }
        if ansi {
            line.push_str("\x1b[0m");
        } else {
            line.truncate(line.trim_end().len());
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// Sequência SGR completa (reset + modificadores + fg + bg).
fn sgr(fg: Color, bg: Color, modifier: Modifier) -> String {
    let mut parts = vec!["0".to_string()];
    let flags = [
        (Modifier::BOLD, "1"),
        (Modifier::DIM, "2"),
        (Modifier::ITALIC, "3"),
        (Modifier::UNDERLINED, "4"),
        (Modifier::SLOW_BLINK, "5"),
        (Modifier::RAPID_BLINK, "6"),
        (Modifier::REVERSED, "7"),
        (Modifier::HIDDEN, "8"),
        (Modifier::CROSSED_OUT, "9"),
    ];
    for (flag, code) in flags {
        if modifier.contains(flag) {
            parts.push(code.to_string());
        }
    }
    if let Some(code) = color_code(fg, false) {
        parts.push(code);
    }
    if let Some(code) = color_code(bg, true) {
        parts.push(code);
    }
    format!("\x1b[{}m", parts.join(";"))
}

/// Código SGR de uma cor (`None` para `Reset`). `bg` desloca 30→40 / 90→100.
fn color_code(color: Color, bg: bool) -> Option<String> {
    let base = |n: u8| Some((if bg { n + 10 } else { n }).to_string());
    let plane = if bg { 48 } else { 38 };
    match color {
        Color::Reset => None,
        Color::Black => base(30),
        Color::Red => base(31),
        Color::Green => base(32),
        Color::Yellow => base(33),
        Color::Blue => base(34),
        Color::Magenta => base(35),
        Color::Cyan => base(36),
        Color::Gray => base(37),
        Color::DarkGray => base(90),
        Color::LightRed => base(91),
        Color::LightGreen => base(92),
        Color::LightYellow => base(93),
        Color::LightBlue => base(94),
        Color::LightMagenta => base(95),
        Color::LightCyan => base(96),
        Color::White => base(97),
        Color::Rgb(r, g, b) => Some(format!("{plane};2;{r};{g};{b}")),
        Color::Indexed(i) => Some(format!("{plane};5;{i}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use ratatui::style::Style;

    #[test]
    fn fixed_clock_is_the_end_of_the_demo_window() {
        assert_eq!(fixed_base().to_rfc3339(), "2026-01-01T12:41:02+00:00");
        assert_eq!((fixed_clock() - fixed_base()).num_milliseconds(), 2843);
    }

    #[test]
    fn prepare_reaches_the_render_once_state() {
        use peabiru_discovery::demo::{self, DemoSensor};
        use peabiru_trace::TraceStatus;
        use std::time::Duration;

        let base = fixed_base();
        let mut app = App::new(
            demo::graph_from(base),
            demo::default_request(),
            demo::environment(),
            crate::Theme::default_dark(),
        );
        let mut sensor = DemoSensor::with_base(Duration::ZERO, base);
        prepare(&mut app, &mut sensor, fixed_clock());
        assert_eq!(app.trace_status(), TraceStatus::Completed);
        assert_eq!(app.events.len(), 7);
        assert_eq!(app.events[0].ts, base);
        assert_eq!(app.fixed_clock, Some(fixed_clock()));
        assert_eq!(
            app.selected_entity().map(|e| e.name.as_str()),
            Some("aws-alb")
        );
        // Determinístico: dois renders iguais, hora em UTC.
        let a = render_to_string(&app, 156, 50, false);
        assert_eq!(a, render_to_string(&app, 156, 50, false));
        assert!(a.contains("12:41:02"), "{a}");
    }

    #[test]
    fn plain_trims_and_skips_wide_cells() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 8, 2));
        buf.set_string(0, 0, "ab", Style::default());
        buf.set_string(0, 1, "コx", Style::default());
        let s = buffer_to_string(&buf, false);
        assert_eq!(s, "ab\nコx\n");
    }

    #[test]
    fn ansi_emits_truecolor_and_reset() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 3, 1));
        buf.set_string(
            0,
            0,
            "a",
            Style::default()
                .fg(Color::Rgb(1, 2, 3))
                .add_modifier(Modifier::BOLD),
        );
        let s = buffer_to_string(&buf, true);
        assert!(s.starts_with("\x1b[0;1;38;2;1;2;3ma"), "{s:?}");
        assert!(s.ends_with("\x1b[0m\n"), "{s:?}");
        assert_eq!(
            sgr(Color::Red, Color::Indexed(7), Modifier::REVERSED),
            "\x1b[0;7;31;48;5;7m"
        );
        assert_eq!(color_code(Color::Reset, true), None);
        assert_eq!(color_code(Color::White, true).as_deref(), Some("107"));
    }
}
