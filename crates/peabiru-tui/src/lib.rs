//! `peabiru-tui`: a interface ratatui do Peabiru (visual dos mockups).
//!
//! Estrutura (ARCHITECTURE §6): `app` (estado), `theme` (cores), `input`
//! (teclas), `run` (loop de terminal), `snapshot` (render para string, base
//! dos testes), `ui` (layout e painéis) e `widgets` (helpers compartilhados
//! pelos painéis).

#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod app;
pub mod input;
pub mod run;
pub mod snapshot;
pub mod theme;
pub mod ui;
pub mod widgets;

pub use app::App;
pub use run::run;
pub use snapshot::render_to_string;
pub use theme::Theme;
