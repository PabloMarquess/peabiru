//! Linha de status (1 linha, sem borda): versão à esquerda, ambiente ao
//! centro, dica de help à direita (ARCHITECTURE §6.2).

use peabiru_discovery::demo::Environment;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

/// Separador entre os campos do ambiente (compacto quando não cabe).
const SEP: &str = "  │  ";
const SEP_COMPACT: &str = " │ ";

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let left = Line::from(Span::styled(
        format!(" peabiru v{}", env!("CARGO_PKG_VERSION")),
        theme.muted(),
    ));
    let right = Line::from(vec![
        Span::styled("press ", theme.muted()),
        Span::styled("?", theme.key()),
        Span::styled(" for help ", theme.muted()),
    ]);
    let [left_area, center_area, right_area] = Layout::horizontal([
        Constraint::Length(cols(left.width())),
        Constraint::Fill(1),
        Constraint::Length(cols(right.width())),
    ])
    .areas(area);

    let mut center = environment_line(theme, &app.environment, SEP);
    if center.width() > usize::from(center_area.width) {
        center = environment_line(theme, &app.environment, SEP_COMPACT);
    }

    frame.render_widget(Paragraph::new(left), left_area);
    frame.render_widget(Paragraph::new(center.centered()), center_area);
    frame.render_widget(Paragraph::new(right.right_aligned()), right_area);
}

/// `environment: prod  │  cluster: prod-e1  │  user: admin`: rótulos em
/// `muted`, valores na cor `title`.
fn environment_line<'a>(theme: &Theme, env: &Environment, sep: &'a str) -> Line<'a> {
    let label = theme.muted();
    let value = Style::default().fg(theme.title);
    Line::from(vec![
        Span::styled("environment: ", label),
        Span::styled(env.environment.clone(), value),
        Span::styled(sep, label),
        Span::styled("cluster: ", label),
        Span::styled(env.cluster.clone(), value),
        Span::styled(sep, label),
        Span::styled("user: ", label),
        Span::styled(env.user.clone(), value),
    ])
}

fn cols(w: usize) -> u16 {
    u16::try_from(w).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use peabiru_discovery::demo;

    #[test]
    fn environment_line_widths() {
        let t = Theme::default_dark();
        let env = demo::environment();
        assert_eq!(environment_line(&t, &env, SEP).width(), 54);
        assert_eq!(environment_line(&t, &env, SEP_COMPACT).width(), 50);
    }
}
