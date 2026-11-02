//! Loop de terminal: raw mode + alternate screen (via `ratatui`), eventos de
//! teclado com timeout de 100 ms e um `on_tick` por intervalo. O terminal é
//! restaurado no drop do guard (inclusive em erro) e no panic hook do ratatui.

use std::time::{Duration, Instant};

use anyhow::Context;
use crossterm::event::{self, Event, KeyEventKind};
use peabiru_discovery::Sensor;

use crate::app::{App, TICK_MS};
use crate::ui;

/// Restaura o terminal ao sair do escopo, aconteça o que acontecer.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

/// Roda a interface até `app.should_quit`.
pub fn run(mut app: App, mut sensor: Box<dyn Sensor>) -> anyhow::Result<()> {
    let mut terminal = ratatui::try_init().context("failed to initialize terminal")?;
    let _guard = TerminalGuard;
    let tick = Duration::from_millis(TICK_MS);
    let mut last_tick = Instant::now();

    while !app.should_quit {
        terminal
            .draw(|frame| ui::draw(frame, &app))
            .context("failed to draw frame")?;

        let timeout = tick.saturating_sub(last_tick.elapsed());
        if event::poll(timeout).context("failed to poll terminal events")? {
            match event::read().context("failed to read terminal event")? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
                _ => {}
            }
        }
        if last_tick.elapsed() >= tick {
            app.on_tick(sensor.as_mut());
            last_tick = Instant::now();
        }
    }
    Ok(())
}
