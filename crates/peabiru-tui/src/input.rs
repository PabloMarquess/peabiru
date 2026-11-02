//! Tabela de teclas (ARCHITECTURE §6.4). Semântica popup-first: com help ou
//! details abertos, a tecla só fecha o popup.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::{App, Tab};

/// Trata uma tecla. `Release` é ignorado (o loop já filtra `Press`).
pub fn handle(app: &mut App, key: KeyEvent) {
    if key.kind == KeyEventKind::Release {
        return;
    }
    let ctrl_c = matches!(key.code, KeyCode::Char('c' | 'C'))
        && key.modifiers.contains(KeyModifiers::CONTROL);

    if app.help_open {
        app.help_open = false;
        return;
    }
    if app.details_open {
        if ctrl_c
            || matches!(
                key.code,
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('d' | 'q')
            )
        {
            app.details_open = false;
        }
        return;
    }
    if ctrl_c {
        app.should_quit = true;
        return;
    }

    match key.code {
        KeyCode::Char(c @ '1'..='5') => {
            if let Some(tab) = Tab::from_digit(c) {
                app.set_tab(tab);
            }
        }
        KeyCode::Char('?') => app.toggle_help(),
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('r') => app.run_trace(),
        KeyCode::Tab => app.next_panel(),
        KeyCode::BackTab => app.prev_panel(),
        KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
        KeyCode::Down | KeyCode::Char('j') => app.select_next(),
        KeyCode::Enter => app.activate_selection(),
        KeyCode::Char('d') => app.toggle_details(),
        KeyCode::Char('y') => app.toggle_yolo(),
        KeyCode::Char('p') => app.toggle_pause(),
        KeyCode::Esc => app.set_tab(Tab::Trace),
        KeyCode::Char('g') => app.select_first(),
        KeyCode::Char('G') => app.select_last(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Panel;
    use crate::theme::Theme;
    use peabiru_discovery::demo;

    fn app() -> App {
        App::new(
            demo::graph(),
            demo::default_request(),
            demo::environment(),
            Theme::default_dark(),
        )
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn tabs_quit_help() {
        let mut app = app();
        handle(&mut app, key(KeyCode::Char('3')));
        assert_eq!(app.tab, Tab::Correlations);
        handle(&mut app, key(KeyCode::Esc));
        assert_eq!(app.tab, Tab::Trace);
        handle(&mut app, key(KeyCode::Char('?')));
        assert!(app.help_open);
        handle(&mut app, key(KeyCode::Char('q')));
        assert!(!app.help_open, "any key closes help");
        assert!(!app.should_quit);
        handle(&mut app, key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_quits_unless_popup() {
        let mut app = app();
        app.details_open = true;
        handle(
            &mut app,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(!app.details_open);
        assert!(!app.should_quit);
        handle(
            &mut app,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(app.should_quit);
    }

    #[test]
    fn details_popup_keys() {
        let mut app = app();
        handle(&mut app, key(KeyCode::Char('r')));
        handle(&mut app, key(KeyCode::Enter));
        assert!(app.details_open, "enter on graph opens details");
        handle(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.selected_node, 0, "keys are swallowed by the popup");
        handle(&mut app, key(KeyCode::Char('d')));
        assert!(!app.details_open);
        handle(&mut app, key(KeyCode::Char('d')));
        assert!(app.details_open);
        handle(&mut app, key(KeyCode::Esc));
        assert!(!app.details_open);
    }

    #[test]
    fn panel_and_selection_keys() {
        let mut app = app();
        handle(&mut app, key(KeyCode::Char('r')));
        handle(&mut app, key(KeyCode::Down));
        assert_eq!(app.selected_node, 1);
        handle(&mut app, key(KeyCode::Char('G')));
        assert_eq!(app.selected_node, 8);
        handle(&mut app, key(KeyCode::Char('g')));
        assert_eq!(app.selected_node, 0);
        handle(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.selected_node, 8);
        handle(&mut app, key(KeyCode::Tab));
        assert_eq!(app.focus, Panel::NodeDetails);
        handle(&mut app, key(KeyCode::BackTab));
        assert_eq!(app.focus, Panel::Graph);
        handle(&mut app, key(KeyCode::Char('p')));
        assert!(app.events_paused);
        handle(&mut app, key(KeyCode::Char('y')));
        assert!(app.yolo);
    }
}
