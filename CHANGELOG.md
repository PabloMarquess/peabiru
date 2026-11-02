# Changelog

## 0.1.0

### Added

- Modelo normalizado, grafo temporal, evidência e confiança (`peabiru-core`).
- Trace engine com caminho por confiança, correlações e saída em texto e JSON. O rótulo de cada trecho mostra o tipo da evidência (`OBSERVED`, `LOG_CONFIRMED`, `DISCOVERED`, ...) e acrescenta `(correlated, N sources)` quando há duas ou mais fontes.
- Provider de demonstração que reproduz os mockups.
- Regras de diagnóstico em Lua, num ambiente isolado, e tema configurável.
- TUI com as abas Trace, Topology, Correlations, Observability e Settings.
- CLI com `tui`, `trace`, `graph` e `rules check`, instalada como `pb` e, com o nome completo, como `peabiru`.
- Lab kind (`labs/kind`) com dez cenários de falha, a sonda do lado A e o gabarito do que o Peabiru deve concluir em cada um.
