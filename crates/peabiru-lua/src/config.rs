//! `config.lua` do usuário: tema (nome → `"#rrggbb"`) e lista de arquivos de
//! regras. O arquivo roda no mesmo sandbox das regras e deve devolver uma
//! tabela:
//!
//! ```lua
//! return {
//!   theme = { observed = "#5fd75f", correlated = "#e5a52e", title = "#5ec8c8" },
//!   rules = { "~/.config/peabiru/rules/*.lua" },
//! }
//! ```
//!
//! Em `rules`, `~` é expandido para o home, caminhos relativos são resolvidos
//! a partir do diretório do `config.lua` e `*`/`?` no **nome do arquivo** são
//! expandidos (ordem alfabética) dentro do diretório indicado.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use mlua::{String as LuaString, Table, Value};
use serde::{Deserialize, Serialize};

use crate::rules::{lua_err, new_sandboxed_lua};

/// Configuração carregada de `config.lua`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Nome do token de cor → `"#rrggbb"`. Chaves desconhecidas são mantidas
    /// (a TUI as reporta em `Theme::apply_overrides`).
    pub theme: BTreeMap<String, String>,
    /// Arquivos de regras, já com `~` expandido e globs resolvidos.
    pub rules: Vec<PathBuf>,
}

/// `$XDG_CONFIG_HOME/peabiru/config.lua` (`dirs::config_dir()`), se houver
/// diretório de configuração.
pub fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("peabiru").join("config.lua"))
}

/// Lê `path` (ou `default_config_path()`). Arquivo ausente (ou nenhum
/// diretório de configuração) dá `Config::default()`; qualquer outro erro de
/// leitura, de Lua ou de formato é devolvido.
pub fn load_config(path: Option<&Path>) -> anyhow::Result<Config> {
    let path = match path.map(Path::to_path_buf).or_else(default_config_path) {
        Some(p) => p,
        None => return Ok(Config::default()),
    };
    let src = match std::fs::read_to_string(&path) {
        Ok(src) => src,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(err) => {
            return Err(err).with_context(|| format!("cannot read config {}", path.display()))
        }
    };
    parse_config(&src, path.parent()).with_context(|| format!("invalid config {}", path.display()))
}

/// Executa o fonte de um `config.lua` no sandbox e converte a tabela
/// devolvida. `base_dir` resolve caminhos relativos em `rules` (normalmente o
/// diretório do arquivo); `None` deixa-os relativos.
pub fn parse_config(src: &str, base_dir: Option<&Path>) -> anyhow::Result<Config> {
    let lua = new_sandboxed_lua()?;
    let value: Value = lua
        .load(src)
        .set_name("=config.lua")
        .eval()
        .map_err(lua_err)?;
    let table = match value {
        Value::Table(t) => t,
        Value::Nil => bail!("config must return a table (got nothing)"),
        other => bail!("config must return a table (got {})", other.type_name()),
    };
    Ok(Config {
        theme: read_theme(&table)?,
        rules: read_rules(&table, base_dir)?,
    })
}

fn lua_str(s: &LuaString) -> anyhow::Result<String> {
    let borrowed = s.to_str().map_err(lua_err)?;
    Ok(String::from(&*borrowed))
}

fn read_theme(table: &Table) -> anyhow::Result<BTreeMap<String, String>> {
    let mut theme = BTreeMap::new();
    let theme_table = match table.raw_get::<Value>("theme").map_err(lua_err)? {
        Value::Nil => return Ok(theme),
        Value::Table(t) => t,
        other => bail!("theme: expected a table, got {}", other.type_name()),
    };
    for pair in theme_table.pairs::<Value, Value>() {
        let (key, value) = pair.map_err(lua_err)?;
        let key = match &key {
            Value::String(s) => lua_str(s)?,
            other => bail!("theme: keys must be strings (got {})", other.type_name()),
        };
        let color = match &value {
            Value::String(s) => lua_str(s)?,
            other => bail!(
                "theme.{key}: expected a color string like \"#rrggbb\", got {}",
                other.type_name()
            ),
        };
        theme.insert(key, color);
    }
    Ok(theme)
}

fn read_rules(table: &Table, base_dir: Option<&Path>) -> anyhow::Result<Vec<PathBuf>> {
    let entries: Vec<String> = match table.raw_get::<Value>("rules").map_err(lua_err)? {
        Value::Nil => Vec::new(),
        Value::String(s) => vec![lua_str(&s)?],
        Value::Table(t) => {
            let mut out = Vec::new();
            for (i, value) in t.sequence_values::<Value>().enumerate() {
                match value.map_err(lua_err)? {
                    Value::String(s) => out.push(lua_str(&s)?),
                    other => bail!(
                        "rules[{}]: expected a path string, got {}",
                        i + 1,
                        other.type_name()
                    ),
                }
            }
            out
        }
        other => bail!("rules: expected a list of paths, got {}", other.type_name()),
    };
    let mut rules = Vec::new();
    for entry in &entries {
        rules.extend(expand_rule_entry(entry, base_dir)?);
    }
    Ok(rules)
}

/// `~` → home; relativo → `base_dir`; `*`/`?` no nome do arquivo → entradas
/// do diretório (só arquivos, ordem alfabética; diretório inexistente → nada).
fn expand_rule_entry(entry: &str, base_dir: Option<&Path>) -> anyhow::Result<Vec<PathBuf>> {
    let entry = entry.trim();
    if entry.is_empty() {
        bail!("rules: empty path");
    }
    let mut path = expand_tilde(entry);
    if path.is_relative() {
        if let Some(base) = base_dir {
            path = base.join(path);
        }
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Ok(vec![path]);
    };
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    if dir.to_str().is_some_and(has_wildcard) {
        bail!("rules: wildcards are only supported in the file name ({entry})");
    }
    if !has_wildcard(name) {
        return Ok(vec![path]);
    }
    let pattern = name.to_string();
    let Ok(read) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut matches: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| wildcard_match(&pattern, n))
        })
        .collect();
    matches.sort();
    Ok(matches)
}

fn has_wildcard(s: &str) -> bool {
    s.contains(['*', '?'])
}

/// `~` e `~/...` → `dirs::home_dir()`; sem home conhecido, fica literal.
fn expand_tilde(entry: &str) -> PathBuf {
    if entry == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    } else if let Some(rest) = entry.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(entry)
}

/// Glob mínimo: `*` casa qualquer sequência, `?` um caractere.
fn wildcard_match(pattern: &str, name: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = name.chars().collect();
    let (mut p, mut i) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None; // (posição do `*`, posição em `s`)
    while i < s.len() {
        if p < pat.len() && (pat[p] == '?' || pat[p] == s[i]) {
            p += 1;
            i += 1;
        } else if p < pat.len() && pat[p] == '*' {
            star = Some((p, i));
            p += 1;
        } else if let Some((sp, si)) = star {
            p = sp + 1;
            i = si + 1;
            star = Some((sp, si + 1));
        } else {
            return false;
        }
    }
    while p < pat.len() && pat[p] == '*' {
        p += 1;
    }
    p == pat.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diretório temporário único por teste, removido no `drop`.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("peabiru-lua-config-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, rel: &str, content: &str) -> PathBuf {
            let p = self.0.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, content).unwrap();
            p
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_file_gives_default() {
        let dir = TempDir::new("missing");
        let cfg = load_config(Some(&dir.path().join("nope.lua"))).unwrap();
        assert_eq!(cfg, Config::default());
        assert!(cfg.theme.is_empty());
        assert!(cfg.rules.is_empty());
    }

    #[test]
    fn default_path_is_under_the_config_dir() {
        if let Some(p) = default_config_path() {
            assert!(p.ends_with("peabiru/config.lua"), "{}", p.display());
        }
    }

    #[test]
    fn theme_and_rules_with_glob_expansion() {
        let dir = TempDir::new("full");
        dir.write("rules/b.lua", "finding('b')");
        dir.write("rules/a.lua", "finding('a')");
        dir.write("rules/notes.txt", "not a rule");
        dir.write("rules/sub/c.lua", "finding('c')");
        let config = dir.write(
            "config.lua",
            r##"
            return {
              theme = { observed = "#5fd75f", correlated = "#e5a52e", bogus = "#000000" },
              rules = { "rules/*.lua", "extra.lua", "rules/?.lua" },
            }
            "##,
        );
        let cfg = load_config(Some(&config)).unwrap();
        assert_eq!(cfg.theme.len(), 3);
        assert_eq!(
            cfg.theme.get("observed").map(String::as_str),
            Some("#5fd75f")
        );
        assert_eq!(cfg.theme.get("bogus").map(String::as_str), Some("#000000"));
        let rules = dir.path().join("rules");
        assert_eq!(
            cfg.rules,
            vec![
                rules.join("a.lua"),
                rules.join("b.lua"),
                dir.path().join("extra.lua"),
                rules.join("a.lua"),
                rules.join("b.lua"),
            ]
        );
    }

    #[test]
    fn tilde_absolute_and_missing_glob_dir() {
        let cfg = parse_config(
            r#"return { rules = { "~/peabiru/x.lua", "/abs/y.lua", "/nonexistent-peabiru-dir/*.lua" } }"#,
            Some(Path::new("/tmp/irrelevant")),
        )
        .unwrap();
        let home_rule = match dirs::home_dir() {
            Some(home) => home.join("peabiru/x.lua"),
            None => PathBuf::from("~/peabiru/x.lua"),
        };
        assert_eq!(cfg.rules, vec![home_rule, PathBuf::from("/abs/y.lua")]);
    }

    #[test]
    fn rules_may_be_a_single_string_and_config_may_omit_return() {
        let cfg = parse_config(r#"{ rules = "a.lua" }"#, None).unwrap();
        assert_eq!(cfg.rules, vec![PathBuf::from("a.lua")]);
        let cfg = parse_config("return {}", None).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn config_runs_in_the_sandbox() {
        let cfg = parse_config(
            r#"return { rules = (os == nil and io == nil and require == nil) and {} or { "leak.lua" } }"#,
            None,
        )
        .unwrap();
        assert!(cfg.rules.is_empty());
    }

    #[test]
    fn invalid_configs_are_errors() {
        assert!(parse_config("return 42", None).is_err());
        assert!(parse_config("", None).is_err());
        assert!(parse_config("return {", None).is_err());
        assert!(parse_config("error('boom')", None).is_err());
        assert!(parse_config(r#"return { theme = "dark" }"#, None).is_err());
        assert!(parse_config(r#"return { theme = { observed = 42 } }"#, None).is_err());
        assert!(parse_config(r#"return { rules = { 1 } }"#, None).is_err());
        assert!(parse_config(r#"return { rules = { "" } }"#, None).is_err());
        assert!(parse_config(r#"return { rules = { "/a/*/b.lua" } }"#, None).is_err());
        let err = parse_config(r#"return { theme = { observed = 42 } }"#, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("theme.observed"), "{err}");
    }

    #[test]
    fn wildcard_matching() {
        assert!(wildcard_match("*.lua", "a.lua"));
        assert!(wildcard_match("*.lua", ".lua"));
        assert!(!wildcard_match("*.lua", "a.lua.bak"));
        assert!(wildcard_match("?.lua", "a.lua"));
        assert!(!wildcard_match("?.lua", "ab.lua"));
        assert!(wildcard_match("a*b*c", "aXXbYYc"));
        assert!(!wildcard_match("a*b*c", "aXXbYY"));
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("**", "anything"));
        assert!(wildcard_match("exact.lua", "exact.lua"));
        assert!(!wildcard_match("exact.lua", "exact.luа"));
    }
}
