# Lab kind do Peabiru

Laboratório local para provar a hipótese do Peabiru antes de ir para a AWS:
**dado "A não fala com B", dizer onde parou e por quê.**

O lab monta o caso "pod falando com banco" em dois sabores (banco dentro do
cluster e um "RDS" fora dele) e injeta uma falha de cada vez. Cada falha tem um
sintoma esperado do lado A, conferido pelo `verify.sh`, e uma conclusão que o
Peabiru vai ter que tirar sozinho, escrita em cada cenário.

Os cenários vêm em **pares com o mesmo sintoma e causas diferentes**. Para quem
depura com `dig`, `nc` ou `nmap`, eles são idênticos. Separar esses pares é o
que o Peabiru tem que fazer melhor que as ferramentas de hoje.

## Topologia

```
 cluster kind                                       rede docker "kind" (o papel da VPC)
 ┌───────────────────────────────────────────┐
 │ nó app                  nó db             │      ┌─────────────────────────────┐
 │ ┌────────────┐ Service  ┌──────────────┐  │ SNAT │ peabiru-lab-rds   (= RDS)   │
 │ │ pod client ├─────────►│ pod postgres │  ├─────►│ + peabiru-lab-rds-fw        │
 │ └─────┬──────┘ClusterIP └──────────────┘  │      │   (nft no netns = SG/NACL)  │
 │       │ DNS                               │      └─────────────────────────────┘
 │       ▼                                   │      ┌─────────────────────────────┐
 │    CoreDNS ── lab.internal ───────────────┼─────►│ peabiru-lab-r53   (= R53)   │
 └───────────────────────────────────────────┘      └─────────────────────────────┘
```

| Na AWS | No lab |
|---|---|
| Pod do EKS | `client` (netshoot) no nó `app` |
| Banco dentro do cluster | `postgres` no nó `db`, atrás do Service `postgres.db` |
| RDS | container `peabiru-lab-rds`, fora do cluster |
| SG / NACL do RDS | regras `nft` no container `peabiru-lab-rds-fw`, que divide o netns do RDS |
| Route 53 (zona privada) e resolver da VPC | container `peabiru-lab-r53`, CoreDNS servindo a zona `lab.internal` com log de consultas |
| CoreDNS do EKS | CoreDNS do kind, encaminhando `lab.internal` para o r53 |

## Segurança

- **kubeconfig isolado** em `.state/kubeconfig`. O lab nunca lê nem altera o
  `~/.kube/config` e não troca o contexto atual. Todo `kubectl` dos scripts
  passa `--kubeconfig` e `--context kind-peabiru-lab`. Isso importa porque o
  contexto atual da máquina pode ser um cluster de cliente.
- Nenhuma regra é aplicada na rede do host. As falhas ficam em objetos do
  cluster kind e dentro do netns do container do RDS.
- Os Postgres rodam sem senha (`trust`) e só são alcançáveis dentro da rede
  docker local.
- `./lab.sh down` remove só o que leva o nome `peabiru-lab`.

## Pré-requisitos

Docker, kind 0.24 ou mais novo (o kindnet aplica NetworkPolicy), kubectl e jq.
Cerca de 3 GB de RAM e 1,5 GB de imagens. Versões e nomes ficam em
[`lab.env`](lab.env).

## Uso

```sh
./lab.sh up              # cria tudo e mostra a linha de base
./scenario.sh list       # cenários, camada e analogia na AWS
./scenario.sh show 07    # sintoma esperado e o que o Peabiru deve concluir
./scenario.sh inject 07  # aplica a falha (aceita prefixo)
./probe.sh               # a visão do lado A: dns, tcp e app nos dois alvos
./scenario.sh revert 07
./verify.sh              # roda a matriz inteira e confere com o esperado
./verify.sh 04 05        # só alguns cenários
./lab.sh status
./lab.sh down
```

A sonda (`probe.sh`) faz o que uma pessoa faria com `dig` e `nc`, sem
inteligência nenhuma. Quando o DNS falha, ela testa o TCP no IP conhecido para
separar "DNS quebrado" de "caminho quebrado". O teste `app` manda o início do
protocolo do Postgres e espera um byte de volta, para provar que o processo do
banco responde, não só o kernel do destino.

## Cenários

| Cenário | Sonda no alvo afetado | Onde está a causa | Par |
|---|---|---|---|
| `00-baseline` | tudo ok | - | |
| `01-dns-nxdomain` | rds: `dns=nxdomain` | registro ausente na zona privada | |
| `02-dns-coredns-down` | ambos: `dns=refused` | kube-dns sem endpoints, recusado no nó de origem | 03 |
| `03-dns-egress-blocked` | ambos: `dns=timeout` | política de saída descarta a porta 53 | 02 |
| `04-svc-no-endpoints` | svc: `tcp=refused` | kube-proxy rejeita no nó de origem (ICMP) | 05, 08 |
| `05-svc-wrong-targetport` | svc: `tcp=refused` | pod do banco responde RST, em outro nó | 04, 08 |
| `06-netpol-ingress-deny` | svc: `tcp=timeout` | política descarta no nó do banco | 07 |
| `07-rds-firewall-drop` | rds: `tcp=timeout` | descarte depois do nó, no "SG" do RDS | 06 |
| `08-rds-port-closed` | rds: `tcp=refused` | RST vindo de fora do cluster | 04, 05 |
| `09-rds-hung` | rds: `app=timeout` | rede inteira ok, processo do banco não responde | |

Na primeira rodada, o `verify.sh` deu 10 de 10 em cerca de 1min40s.

### Prova de que os pares diferem abaixo do `nc`

Capturado de dentro do pod cliente. Para o `nc`, os dois dizem
`Connection refused`:

```
04-svc-no-endpoints
  10.244.2.3 > 10.96.224.57.5432: Flags [S]
  10.244.2.1 > 10.244.2.3: ICMP 10.96.224.57 tcp port 5432 unreachable   <- o próprio nó

05-svc-wrong-targetport
  10.244.2.3 > 10.96.224.57.5432: Flags [S]
  10.96.224.57.5432 > 10.244.2.3: Flags [R.]                             <- "o Service"
```

No 05, o RST parece vir do ClusterIP porque o NAT reverso esconde o pod que
respondeu. Desfazer essa tradução (conntrack) e apontar "o RST veio do pod
`postgres-…` no nó `db`, porta 5433 fechada" é trabalho do Peabiru.

## Regra do jogo para o Peabiru

Os containers `peabiru-lab-rds`, `peabiru-lab-rds-fw` e `peabiru-lab-r53` são
"a AWS". **O Peabiru não pode instrumentar o netns deles**, porque na AWS o RDS
e a malha da VPC são caixa-preta. No kind tudo roda no mesmo kernel e o eBPF
conseguiria enxergar lá dentro; o teste só vale se o agente se limitar aos nós
do cluster. Do lado de lá, o Peabiru só pode usar a resposta da sonda e, num
passo futuro, uma "API AWS falsa" (abaixo).

## Diferenças em relação à AWS

- **Mesmo kernel para tudo.** Na AWS, cada nó tem o seu e o RDS é inacessível.
- **SNAT no caminho para o RDS.** O kindnet mascara o tráfego que sai do
  cluster. No EKS com VPC CNI, para um RDS na mesma VPC, não há SNAT. Aqui o
  SNAT serve para treinar a correlação via conntrack.
- **Sem Flow Logs nem API de Security Group.** As regras do `rds-fw` e o log de
  consultas do `r53` existem, mas ainda não são expostos como a AWS exporia.

## Observações

- O `kind load docker-image` falha com o armazenamento de imagens do Docker 29
  (ele tenta importar todas as plataformas). O `lab.sh` carrega as imagens com
  `docker save --platform` + `ctr import`, só na plataforma da máquina.
- Na primeira subida, a primeira conexão ao Service levou cerca de 1024 ms, que
  é o tempo de reenvio do primeiro SYN. Não se repetiu. É o tipo de evento que o
  Peabiru deve flagrar (SYN reenviado logo depois do pod ficar pronto).

## Próximos passos

1. Agente eBPF mínimo, só observando, nos nós do cluster: mudança de estado TCP,
   retransmissão, RST recebido, descarte com motivo (`kfree_skb`) e DNS.
2. Conntrack para ligar o antes e o depois do NAT.
3. "API AWS falsa": publicar as regras do `rds-fw` como se fossem um Security
   Group e o log do `r53` como o log de consultas do Route 53 Resolver, para o
   provider AWS ser testado aqui antes da conta real.
4. Cenário de resolver da zona fora do ar (CoreDNS sem conseguir falar com o
   r53), comum no EKS quando o SG dos nós bloqueia o resolver da VPC.
