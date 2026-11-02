<p align="center">
  <img src="docs/brand/peabiru-banner.svg" width="720" alt="Emblema do Peabiru em pixel art: uma bússola com sol nascente, araucária e uma trilha de terra que leva até uma pegada de onça, ao lado do nome peabiru.">
</p>

# Peabiru

Peabiru é uma ferramenta de troubleshooting e observabilidade de rede que
reconstrói e explica a jornada completa de uma conexão, do processo de origem
ao processo de destino, atravessando pods, proxies, gateways, NATs, load
balancers e clouds. Em vez de girar em torno da captura de pacotes, ela é
construída ao redor de um **grafo global de rede**: discovery (AWS, Kubernetes,
…) fornece fatos topológicos, sensores (eBPF, flow logs, probes) fornecem
observações, a correlação liga os dois, o trace engine procura o caminho entre
A e B e o diagnostic engine explica o que parece errado nele. Toda aresta
carrega evidência e confiança: nada inferido é exibido como observado.

O nome vem do Caminho do Peabiru, a antiga rede de trilhas que ligava regiões
da América do Sul. A visão completa do produto está em
[`docs/VISION.md`](docs/VISION.md); o contrato técnico entre os crates (tipos,
assinaturas, layout da interface, fixture de demonstração) está em
[`ARCHITECTURE.md`](ARCHITECTURE.md).

## Status

**Base v0.1: TUI + provider demo; sem eBPF/AWS/K8s ainda.** O grafo é
populado por uma fixture que reproduz os mockups (um pod `web-frontend`
chegando a `api.prod.svc.cluster.local` via kube-proxy, Istio, ALB e
`app-svc`), o trace engine, o motor de confiança, as correlações, as regras
Lua e a interface já funcionam sobre ela.

## Build e execução

Requer Rust 1.88+ (edição 2021). O Lua 5.4 é compilado junto (`mlua`
vendored), sem dependências de sistema.

```sh
cargo build --release                 # binários em target/release/pb e target/release/peabiru

cargo run -p peabiru-cli              # = pb tui (interface interativa)
cargo run -p peabiru-cli -- trace --from web-frontend-7f9c8c --to api.prod.svc.cluster.local
cargo run -p peabiru-cli -- trace --from web-frontend-7f9c8c --to api.prod.svc.cluster.local --json
cargo run -p peabiru-cli -- graph     # dump do grafo (--json para JSON)
cargo run -p peabiru-cli -- rules check rules/default.lua
cargo run -p peabiru-cli -- tui --render-once --size 156x50 --ansi   # um frame, sem terminal

cargo test --workspace               # inclui os snapshots do Tab Trace (156x50 e 100x30)
```

`--render-once` é determinístico (grafo e eventos do demo numa base fixa,
`2026-01-01T12:41:02Z`, horas em UTC): é o frame de referência que
`crates/peabiru-tui/tests/snapshot.rs` compara com
`crates/peabiru-tui/tests/snapshots/trace_<WxH>.txt`. Para regenerar os
arquivos depois de uma mudança visual intencional:

```sh
cargo run -q -p peabiru-cli -- tui --render-once --size 156x50 > crates/peabiru-tui/tests/snapshots/trace_156x50.txt
cargo run -q -p peabiru-cli -- tui --render-once --size 100x30 > crates/peabiru-tui/tests/snapshots/trace_100x30.txt
```

(ou `PEABIRU_UPDATE_SNAPSHOTS=1 cargo test -p peabiru-tui --test snapshot`).

Subcomandos e opções:

```
pb                                       # = pb tui
pb tui [--provider demo] [--render-once --size WxH [--ansi]]
pb trace --from <selector> --to <selector|fqdn> [--port 443] [--protocol HTTPS] [--json]
pb graph [--json]
pb rules check <FILE>...
pb --version
```

`pb` é o comando principal. `peabiru` é o mesmo comando com o nome completo:
`cargo install --path crates/peabiru-cli` instala os dois, e a ajuda mostra o
nome que você digitou.

Opções globais (antes ou depois do subcomando): `--config PATH` (config.lua),
`--rules PATH` (arquivo de regras extra, repetível) e `--no-bg` (não pinta o
fundo, mantém a cor do terminal). Um `<selector>` é o id da entidade
(`k8s://prod/pod/web-frontend-7f9c8c`), o nome (`web-frontend-7f9c8c`), um IP,
`ip:porta` ou hostname.

Códigos de saída: `0` ok, `1` erro de execução, `2` erro de argumento, `3`
trace falhou (origem/destino não encontrado ou sem caminho).

## Teclas da interface

| tecla | ação |
|---|---|
| `1`–`5` | troca de tab (Trace, Topology, Correlations, Observability, Settings) |
| `?` | abre/fecha a ajuda |
| `q` / `Ctrl-C` | sair (com popup aberto, só fecha o popup) |
| `r` | executa o trace |
| `Tab` / `Shift-Tab` | próximo/anterior painel focado |
| `↑`/`↓`, `k`/`j` | navega no painel focado (grafo, correlações, eventos) |
| `Enter` | grafo: abre detalhes; correlações: seleciona o nó `a` no grafo |
| `d` | abre/fecha os detalhes do nó selecionado |
| `y` | alterna o yolo mode (trace contínuo, reinicia a cada 3 s) |
| `p` | pausa/retoma o stream de eventos |
| `Esc` | fecha popup; senão volta ao tab Trace |
| `g` / `G` | primeiro/último item do painel focado |

Tamanho mínimo do terminal: 100×30 (referência dos mockups: 156×50).

## Configuração (`config.lua`)

Lido de `$XDG_CONFIG_HOME/peabiru/config.lua` (normalmente
`~/.config/peabiru/config.lua`) ou do caminho dado em `--config`. Arquivo
ausente = padrões.

```lua
return {
  theme = { observed = "#5fd75f", correlated = "#e5a52e", title = "#5ec8c8" },
  rules = { "~/.config/peabiru/rules/*.lua" },
}
```

Cores aceitas em `theme` (`"#rrggbb"`): `bg`, `panel_bg`, `border`,
`border_focus`, `title`, `text`, `muted`, `key`, `selection`, `logo`,
`logo_sub`, `observed`, `inferred`, `correlated`, `probe`, `ok`, `warn`,
`error`. Chaves desconhecidas e cores inválidas geram um aviso em stderr e
são ignoradas. Em `rules`, `~` é expandido, caminhos relativos são resolvidos
a partir do diretório do `config.lua` e `*`/`?` valem no nome do arquivo.

## Regras de diagnóstico (Lua)

As regras rodam **fora do datapath**: só sobre o resultado de um trace, num
sandbox Lua 5.4 (sem `os`, `io`, `package`, `require`, `load`). Cada arquivo
é executado uma vez por trace, num ambiente próprio. As regras padrão estão em
[`rules/default.lua`](rules/default.lua); arquivos extras entram por
`config.lua` ou `--rules`, e `pb rules check arquivo.lua` executa-os
(junto com as padrão) contra o trace demo e imprime `[SEVERIDADE] regra:
mensagem` por finding (no demo, `[WARNING] default.lua: Possible traffic
tromboning`).

```lua
finding(message [, severity [, subject]])   -- severity: "info" | "warning" | "critical" (padrão "warning")
trace.confidence                             -- 0..100
trace.hops, trace.observed, trace.inferred, trace.correlations
flow.rtt_ms, flow.retransmissions, flow.drops, flow.rst   -- agregados do caminho (RTT máx., somas)
path:contains(x)                             -- x = kind ("nat", "load_balancer", ...) ou nome de nó
path.hops[i]                                 -- { name, kind, status, address, confidence, rtt_ms, retransmissions, drops, rst }
source / destination                         -- { name, ip, kind }, com :is_private()
```

Exemplo:

```lua
if source:is_private() and path:contains("nat") and destination:is_private() then
  finding("Possible traffic tromboning")
end
```

Um erro numa regra vira um finding `warning` (`rule error: ...`) sem impedir
as demais. Os findings aparecem no `trace` (texto e JSON) e no tab
Observability da interface.

## Crates

```
crates/
├── peabiru-core/        # modelo normalizado, grafo temporal, evidência/confiança, eventos, findings
├── peabiru-trace/       # trace engine: path finding (Dijkstra por confiança), correlações, render ASCII
├── peabiru-discovery/   # traits Provider/Sensor, registro e o provider "demo" (fixture dos mockups)
├── peabiru-lua/         # runtime Lua sandboxed: regras de diagnóstico + config/tema
├── peabiru-tui/         # interface ratatui (layout dos mockups, snapshot para testes)
└── peabiru-cli/         # comandos `pb` e `peabiru`
```

`peabiru-core` não depende de nenhum outro crate nem de provider específico;
`peabiru-tui` não depende de Lua: os findings chegam pelo CLI.

## Lab local

[`labs/kind`](labs/kind/README.md) monta o caso "pod falando com banco" num
cluster kind, com um "RDS" e um "Route 53" fora dele, e injeta dez falhas
diferentes. É o gabarito: um diagnóstico do Peabiru só está pronto quando acerta
os cenários de lá.

## Desenvolvimento

Requer Rust (a versão vem de `rust-toolchain.toml`) e Docker, que roda as
ferramentas externas fixadas por digest.

```sh
git config core.hooksPath .githooks   # uma vez por clone
sh scripts/check.sh                   # o que o pre-push e o CI rodam
```

| Verificação | Ferramenta |
|---|---|
| Formato e lint | `cargo fmt`, `cargo clippy -D warnings` |
| Testes | `cargo test --workspace --locked` |
| Dependências | `cargo audit` (RustSec), Dependabot semanal com 7 dias de espera |
| Segredos, histórico inteiro | gitleaks |
| Scripts | shellcheck |
| Workflows | actionlint, zizmor |
| Mensagens de commit | Conventional Commits, sem atribuição de IA |

Releases seguem [SemVer](https://semver.org): a tag `v<x.y.z>` publica a release
com as notas do [`CHANGELOG.md`](CHANGELOG.md). Vulnerabilidades: veja
[`SECURITY.md`](SECURITY.md).

## Roadmap

- Providers reais: `aws` (VPC, ENI, route tables, ALB/NLB, NAT, TGW, flow
  logs), `kubernetes` (Services, EndpointSlices, Pods, CNI), `linux-ebpf`
  (conexões, retransmissões, drops por processo/cgroup).
- Probes ativos (TCP/HTTP/DNS) para confirmar trechos inferidos.
- Entity resolution e correlação entre fontes (5-tuple, cgroup, flow logs).
- Exportação OTLP e correlação com application tracing.
- Multicloud (Azure, GCP, Magalu) e on-prem sem reescrever o core.

## Licença

Apache-2.0.
