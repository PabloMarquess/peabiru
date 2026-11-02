//! Tema de cores (ARCHITECTURE §6.5). Os nomes aceitos em `config.lua` são
//! os de `Theme::names()`; valores são `"#rrggbb"`.

use std::collections::BTreeMap;

use peabiru_core::NodeStatus;
use ratatui::style::{Color, Modifier, Style};

/// Paleta da interface. Cores padrão = mockup 1 (`default_dark`).
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub bg: Color,
    pub panel_bg: Color,
    pub border: Color,
    pub border_focus: Color,
    pub title: Color,
    pub text: Color,
    pub muted: Color,
    pub key: Color,
    pub selection: Color,
    pub logo: Color,
    pub logo_sub: Color,
    pub observed: Color,
    pub inferred: Color,
    pub correlated: Color,
    pub probe: Color,
    pub ok: Color,
    pub warn: Color,
    pub error: Color,
    /// `false` com `--no-bg`: não pinta `bg`/`panel_bg`.
    pub paint_bg: bool,
}

/// Nomes de cor aceitos por `apply_overrides` / `config.lua`.
const NAMES: &[&str] = &[
    "bg",
    "panel_bg",
    "border",
    "border_focus",
    "title",
    "text",
    "muted",
    "key",
    "selection",
    "logo",
    "logo_sub",
    "observed",
    "inferred",
    "correlated",
    "probe",
    "ok",
    "warn",
    "error",
];

impl Default for Theme {
    fn default() -> Self {
        Self::default_dark()
    }
}

impl Theme {
    /// Paleta escura dos mockups.
    pub fn default_dark() -> Self {
        Self {
            bg: Color::Rgb(0x0b, 0x0e, 0x14),
            panel_bg: Color::Rgb(0x0b, 0x0e, 0x14),
            border: Color::Rgb(0x2b, 0x32, 0x40),
            border_focus: Color::Rgb(0xe5, 0xb4, 0x2e),
            title: Color::Rgb(0x5e, 0xc8, 0xc8),
            text: Color::Rgb(0xc8, 0xd0, 0xda),
            muted: Color::Rgb(0x7c, 0x85, 0x94),
            key: Color::Rgb(0xe5, 0xb4, 0x2e),
            selection: Color::Rgb(0x1c, 0x23, 0x30),
            logo: Color::Rgb(0xf2, 0xb1, 0x34),
            logo_sub: Color::Rgb(0x5e, 0xc8, 0xc8),
            observed: Color::Rgb(0x5f, 0xd7, 0x5f),
            inferred: Color::Rgb(0xc8, 0xd0, 0xda),
            correlated: Color::Rgb(0xe5, 0xa5, 0x2e),
            probe: Color::Rgb(0xa7, 0x8b, 0xfa),
            ok: Color::Rgb(0x5f, 0xd7, 0x5f),
            warn: Color::Rgb(0xe5, 0xb4, 0x2e),
            error: Color::Rgb(0xf0, 0x6c, 0x6c),
            paint_bg: true,
        }
    }

    /// Aplica `nome → "#rrggbb"`. Devolve as entradas rejeitadas: chaves
    /// desconhecidas (o próprio nome) e valores que não são `#rrggbb`
    /// (`"<chave>: invalid color '<valor>'"`).
    pub fn apply_overrides(&mut self, overrides: &BTreeMap<String, String>) -> Vec<String> {
        let mut rejected = Vec::new();
        for (key, value) in overrides {
            let Some(slot) = self.slot_mut(key) else {
                rejected.push(key.clone());
                continue;
            };
            match parse_hex_color(value) {
                Some(color) => *slot = color,
                None => rejected.push(format!("{key}: invalid color '{value}'")),
            }
        }
        rejected
    }

    /// Cor associada a um `NodeStatus`.
    pub fn status_color(&self, s: NodeStatus) -> Color {
        match s {
            NodeStatus::Observed => self.observed,
            NodeStatus::Inferred => self.inferred,
            NodeStatus::Correlated => self.correlated,
            NodeStatus::Probe => self.probe,
        }
    }

    /// Cor de um score/confiança em %: `>= 80` ok, `>= 60` warn, senão error.
    pub fn score_color(&self, pct: u8) -> Color {
        match pct {
            80..=u8::MAX => self.ok,
            60..=79 => self.warn,
            _ => self.error,
        }
    }

    /// Nomes aceitos no `config.lua` (mesma ordem da struct).
    pub fn names() -> &'static [&'static str] {
        NAMES
    }

    /// Cor pelo nome (`None` se desconhecido).
    pub fn get(&self, name: &str) -> Option<Color> {
        match name {
            "bg" => Some(self.bg),
            "panel_bg" => Some(self.panel_bg),
            "border" => Some(self.border),
            "border_focus" => Some(self.border_focus),
            "title" => Some(self.title),
            "text" => Some(self.text),
            "muted" => Some(self.muted),
            "key" => Some(self.key),
            "selection" => Some(self.selection),
            "logo" => Some(self.logo),
            "logo_sub" => Some(self.logo_sub),
            "observed" => Some(self.observed),
            "inferred" => Some(self.inferred),
            "correlated" => Some(self.correlated),
            "probe" => Some(self.probe),
            "ok" => Some(self.ok),
            "warn" => Some(self.warn),
            "error" => Some(self.error),
            _ => None,
        }
    }

    fn slot_mut(&mut self, name: &str) -> Option<&mut Color> {
        match name {
            "bg" => Some(&mut self.bg),
            "panel_bg" => Some(&mut self.panel_bg),
            "border" => Some(&mut self.border),
            "border_focus" => Some(&mut self.border_focus),
            "title" => Some(&mut self.title),
            "text" => Some(&mut self.text),
            "muted" => Some(&mut self.muted),
            "key" => Some(&mut self.key),
            "selection" => Some(&mut self.selection),
            "logo" => Some(&mut self.logo),
            "logo_sub" => Some(&mut self.logo_sub),
            "observed" => Some(&mut self.observed),
            "inferred" => Some(&mut self.inferred),
            "correlated" => Some(&mut self.correlated),
            "probe" => Some(&mut self.probe),
            "ok" => Some(&mut self.ok),
            "warn" => Some(&mut self.warn),
            "error" => Some(&mut self.error),
            _ => None,
        }
    }

    // --- estilos prontos -----------------------------------------------

    /// Título de painel: cor `title`, negrito.
    pub fn title(&self) -> Style {
        Style::default().fg(self.title).add_modifier(Modifier::BOLD)
    }

    /// Texto normal.
    pub fn text(&self) -> Style {
        Style::default().fg(self.text)
    }

    /// Texto secundário (rótulos, endereços).
    pub fn muted(&self) -> Style {
        Style::default().fg(self.muted)
    }

    /// Tecla de atalho: cor `key`, negrito.
    pub fn key(&self) -> Style {
        Style::default().fg(self.key).add_modifier(Modifier::BOLD)
    }

    /// Borda de painel (`border_focus` quando focado).
    pub fn border(&self, focused: bool) -> Style {
        Style::default().fg(if focused {
            self.border_focus
        } else {
            self.border
        })
    }

    /// Estilo de texto na cor do status.
    pub fn status(&self, s: NodeStatus) -> Style {
        Style::default().fg(self.status_color(s))
    }

    /// Fundo da linha selecionada.
    pub fn selection(&self) -> Style {
        Style::default().bg(self.selection)
    }
}

/// `"#rrggbb"` (ou `"#rgb"`, maiúsculas/minúsculas, `#` opcional) → `Color::Rgb`.
pub fn parse_hex_color(s: &str) -> Option<Color> {
    let hex = s.trim().strip_prefix('#').unwrap_or(s.trim());
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        6 => Some(Color::Rgb(channel(0)?, channel(2)?, channel(4)?)),
        3 => {
            let nib = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|v| v * 17);
            Some(Color::Rgb(nib(0)?, nib(1)?, nib(2)?))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_colors() {
        assert_eq!(
            parse_hex_color("#5fd75f"),
            Some(Color::Rgb(0x5f, 0xd7, 0x5f))
        );
        assert_eq!(
            parse_hex_color("E5B42E"),
            Some(Color::Rgb(0xe5, 0xb4, 0x2e))
        );
        assert_eq!(parse_hex_color("#fff"), Some(Color::Rgb(255, 255, 255)));
        assert_eq!(parse_hex_color("#12345"), None);
        assert_eq!(parse_hex_color("red"), None);
        assert_eq!(parse_hex_color(""), None);
    }

    #[test]
    fn overrides_apply_and_report_unknown() {
        let mut theme = Theme::default_dark();
        let mut o = BTreeMap::new();
        o.insert("observed".to_string(), "#112233".to_string());
        o.insert("bogus".to_string(), "#000000".to_string());
        o.insert("title".to_string(), "not-a-color".to_string());
        let rejected = theme.apply_overrides(&o);
        assert_eq!(theme.observed, Color::Rgb(0x11, 0x22, 0x33));
        assert_eq!(theme.title, Theme::default_dark().title);
        assert_eq!(
            rejected,
            vec![
                "bogus".to_string(),
                "title: invalid color 'not-a-color'".to_string()
            ]
        );
    }

    #[test]
    fn names_cover_every_slot() {
        let mut theme = Theme::default_dark();
        for name in Theme::names() {
            assert!(theme.get(name).is_some(), "{name}");
            assert!(theme.slot_mut(name).is_some(), "{name}");
        }
        assert_eq!(Theme::names().len(), 18);
    }

    #[test]
    fn status_and_score_colors() {
        let t = Theme::default_dark();
        assert_eq!(t.status_color(NodeStatus::Observed), t.observed);
        assert_eq!(t.status_color(NodeStatus::Correlated), t.correlated);
        assert_eq!(t.status_color(NodeStatus::Probe), t.probe);
        assert_eq!(t.score_color(92), t.ok);
        assert_eq!(t.score_color(73), t.warn);
        assert_eq!(t.score_color(12), t.error);
        assert_eq!(t.border(true).fg, Some(t.border_focus));
        assert_eq!(t.border(false).fg, Some(t.border));
    }
}
