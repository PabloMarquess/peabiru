# peabiru

Diagnóstico de caminho de rede em Rust e eBPF. Repositório público.

- Commits seguem Conventional Commits: assunto curto, sem corpo salvo necessidade, sem atribuição de IA.
- Rode `sh scripts/check.sh` antes de commitar. Nunca contorne os hooks com `--no-verify`.
- `ARCHITECTURE.md` é o contrato entre os crates: quem muda uma interface pública atualiza o arquivo no mesmo commit.
- Sem comentários no código, a não ser quando o porquê não é óbvio.
- Dependências só de alta adesão e mantidas: milhões de downloads, lançamento recente e nada aberto no RustSec. Uma dependência nova é justificada no commit que a adiciona.
- Ferramentas externas (gitleaks, shellcheck, actionlint, zizmor) rodam como imagens Docker fixadas por digest; actions do GitHub, por SHA.
- Os programas eBPF são declarados GPL, porque o kernel exige isso para os helpers de leitura; o resto do projeto é Apache-2.0. Eles só observam: nenhum programa descarta, altera ou redireciona pacotes.
- O lab em `labs/kind` é o gabarito: um diagnóstico só está pronto quando acerta os cenários de lá. Nada no projeto usa o `~/.kube/config` nem o contexto atual do kubectl.
- Código derivado de projetos de terceiros mantém o aviso de copyright em `NOTICE.md`.
- Releases: suba `version` em `[workspace.package]` do `Cargo.toml`, adicione a entrada no `CHANGELOG.md`, faça o commit `chore: release v<x.y.z>` e crie a tag `v<x.y.z>`. O workflow publica a release a partir do changelog.
