# Projeto Peabiru — End-to-End Network Path Intelligence

> Documento de visão original do projeto. A base implementada está descrita em
> `../ARCHITECTURE.md`. A seção final ("Solicitação para a próxima LLM") é uma
> pauta aberta de análise crítica, ainda não executada.

## 1. Visão do projeto

Peabiru é uma ferramenta de troubleshooting e observabilidade de rede cujo objetivo é reconstruir e explicar a jornada completa de uma conexão, desde o processo de origem até o processo de destino.

O nome faz referência ao Caminho do Peabiru, uma antiga rede de caminhos que conectava diferentes regiões da América do Sul. A metáfora do projeto é justamente essa: descobrir e representar caminhos que atravessam diferentes ambientes, clouds, redes, clusters e componentes.

O Peabiru NÃO deve ser apenas um novo tcpdump.

A ideia é criar uma categoria mais ampla de ferramenta:

> End-to-End Network Path Intelligence Engine

Ela deve conseguir responder:

"Como uma conexão saiu do ponto A, por onde deveria passar, por onde conseguimos confirmar que passou, onde terminou e onde provavelmente existe um problema?"

---

# 2. Problema que o Peabiru tenta resolver

Hoje diferentes ferramentas enxergam partes isoladas do problema:

tcpdump:
- pacotes

MTR/traceroute:
- hops
- latência aproximada
- perda aparente

eBPF:
- sockets
- processos
- kernel
- RTT
- retransmissões
- drops
- datapath local

pwru:
- trajetória dentro do kernel

Cilium/Hubble:
- datapath e flows Kubernetes

Cloud APIs:
- topologia e configuração

Kubernetes API:
- Services
- Pods
- EndpointSlices
- Ingress
- NetworkPolicies

OpenTelemetry:
- application/distributed tracing

O Peabiru deve correlacionar essas diferentes perspectivas.

Exemplo:

```
Processo EC2
    ↓
EC2
    ↓
ENI
    ↓
Route Table
    ↓
Transit Gateway
    ↓
Inspection VPC
    ↓
Network Firewall
    ↓
Load Balancer
    ↓
Ingress
    ↓
Kubernetes Service
    ↓
EndpointSlice
    ↓
Pod
    ↓
Processo
```

A ferramenta deve conseguir diferenciar aquilo que foi realmente observado daquilo que foi inferido a partir da configuração.

---

# 3. Princípio central

O Peabiru deve ser construído ao redor de um:

> Global Network Graph

e NÃO ao redor da captura de pacotes.

Diversas fontes adicionam fatos ao grafo.

Discovery:
"Quais recursos existem e como estão relacionados?"

Sensors:
"O que realmente aconteceu?"

Correlation:
"Esses fatos representam a mesma entidade ou conexão?"

Trace Engine:
"Qual é o caminho entre A e B?"

Diagnostic Engine:
"O que parece estar errado nesse caminho?"

---

# 4. Arquitetura conceitual

```
                    User / CLI / API / UI
                            │
                            ▼
                     ┌──────────────┐
                     │ Trace Engine │
                     └──────┬───────┘
                            │
                            ▼
                ┌───────────────────────┐
                │  Correlation Engine   │
                │                       │
                │ Entity Resolution     │
                │ Evidence Correlation  │
                │ Path Correlation      │
                └───────────┬───────────┘
                            │
                            ▼
                 ┌─────────────────────┐
                 │ GLOBAL NETWORK GRAPH│
                 └──────────▲──────────┘
                            │
           ┌────────────────┼─────────────────┐
           │                │                 │
           ▼                ▼                 ▼

        Sensors          Discovery         Telemetry

     eBPF/Linux          AWS               OpenTelemetry
     Netlink             Azure
     Active Probes       GCP
     Packet Capture      Magalu
                         Kubernetes
                         VMware
                         OpenStack
                         On-Prem
                         Network Devices
```

---

# 5. Sensor Layer

Sensores representam evidência observada.

A primeira implementação deve focar principalmente em Linux/eBPF.

Possíveis eventos:

- TCP connect
- TCP close
- TCP state
- RTT
- retransmissions
- RST
- drops
- socket
- PID
- process
- cgroup
- container
- Kubernetes Pod
- network namespace
- interface
- routing
- conntrack
- NAT
- TC
- XDP

Exemplo:

```json
{
    "src": "10.1.4.23:49122",
    "dst": "10.50.20.15:443",
    "pid": 2821,
    "process": "java",
    "rtt_ms": 18.4,
    "retransmissions": 2
}
```

O Peabiru não deve gerar um evento para cada pacote por padrão.

Preferência:

```
packets
   ↓
eBPF
   ↓
flow aggregation
   ↓
connection events
   ↓
correlation
```

Packet capture detalhado deve ser ativado apenas quando solicitado.

---

# 6. Agente

Pode existir um agente pequeno rodando nos hosts Linux.

Em VMs/EC2:

```
Application
    ↓
Socket
    ↓
Kernel
    ↓
eBPF
    ↓
NIC
```

Em Kubernetes:

```
Pods
 ↓
Linux Kernel
 ↓
Peabiru Agent
 ↓
CNI / NIC
```

Em Kubernetes ele poderia rodar como DaemonSet, sem sidecar por Pod.

O agente NÃO deve possuir toda a inteligência do sistema.

Ele deve preferencialmente:

- coletar eventos locais;
- fazer agregação;
- identificar contexto local;
- enviar evidências para o control plane.

Discovery de AWS, cloud e Kubernetes deve ficar majoritariamente centralizado.

---

# 7. Agentless

O produto também deve conseguir operar sem agente.

Nesse modo pode utilizar:

- Cloud APIs
- Kubernetes API
- route tables
- Flow Logs
- Firewall logs
- Load Balancer logs
- Active probes

Agentless produz principalmente:

> caminho inferido

Agent/eBPF acrescenta:

> caminho observado

Os dois modos devem coexistir.

---

# 8. Discovery

Discovery NÃO significa capturar tráfego.

Discovery significa:

> entender o ambiente e alimentar o grafo com entidades e relações.

Não existe direção no Discovery.

Não deve existir algo como:

```
Kubernetes Discovery
       ↓
AWS Discovery
```

Em vez disso:

```
AWS Account A ───┐
AWS Account B ───┤
Azure ───────────┤
GCP ─────────────┤
Magalu ──────────┤
Kubernetes A ────┼──► Global Graph
Kubernetes B ────┤
On-Prem ─────────┤
Linux/eBPF ──────┘
```

Depois o Trace Engine consulta esse grafo em qualquer direção.

---

# 9. AWS Discovery

AWS deve ser apenas um provider, não uma dependência estrutural do Core.

Pode descobrir:

- Accounts
- EC2
- ENIs
- VPCs
- Subnets
- Route Tables
- Security Groups
- NACLs
- IGWs
- NAT Gateways
- Transit Gateways
- TGW route tables
- TGW attachments
- VPN
- Direct Connect
- VPC Endpoints
- PrivateLink
- ALB
- NLB
- Target Groups
- Network Firewall
- WAF
- API Gateway
- demais componentes relevantes

Exemplo:

```
10.0.8.32
   ↓
ENI
   ↓
Load Balancer
   ↓
Listener 443
   ↓
Target Group
```

---

# 10. Kubernetes Discovery

Deve utilizar a Kubernetes API e ser praticamente independente do local em que Kubernetes está instalado.

Pode funcionar com:

- EKS
- AKS
- GKE
- Magalu Kubernetes
- OpenShift
- Rancher
- Kubernetes bare metal
- Kubernetes em VMware/OpenStack

Descobrir:

- Cluster
- Nodes
- Pods
- Services
- EndpointSlices
- Ingress
- NetworkPolicies
- Namespaces
- labels
- IPs
- ports

Exemplo:

```
Service
   ↓
EndpointSlice
   ↓
Pod
   ↓
Node
```

---

# 11. Multicloud

O Core NÃO deve possuir tipos específicos de AWS espalhados pelo código.

Criar uma Normalization Layer.

Providers específicos:

```
AWS Transit Gateway
Azure Gateway
Router físico
OpenStack Router
```

podem ser normalizados para conceitos comuns.

Modelo genérico inicial:

```
Host
Interface
IP
Network
Subnet
Route
Gateway
Firewall
LoadBalancer
NAT
Tunnel
Endpoint
Cluster
Node
Service
Workload
Process
```

Providers:

```
providers/
    aws/
    azure/
    gcp/
    magalu/
    kubernetes/
    linux/
    vmware/
    openstack/
    network/
```

Todos escrevem no mesmo grafo.

---

# 12. Exemplo multicloud

```
AWS Account X
      ↓
EC2
      ↓
Route
      ↓
VPN / Internet / private interconnect
      ↓
Magalu Cloud
      ↓
Load Balancer
      ↓
Kubernetes
      ↓
Service
      ↓
EndpointSlice
      ↓
Pod
```

Providers envolvidos:

```
AWS Discovery
Magalu Discovery
Kubernetes Discovery
Linux/eBPF
```

O Trace Engine deve tratar isso como um único caminho.

---

# 13. Entity Resolution

Uma parte crítica do projeto será descobrir que diferentes sistemas estão descrevendo a mesma entidade.

Exemplo:

AWS:

```
EC2 i-abcd
ENI eni-123
IP 10.20.4.8
```

Kubernetes:

```
Node ip-10-20-4-8
InternalIP 10.20.4.8
```

eBPF:

```
Host 10.20.4.8
```

O sistema deve concluir:

```
AWS EC2
    ==
Kubernetes Node
    ==
Linux Host
```

Entity Resolution provavelmente será uma das partes mais importantes do Peabiru.

Possíveis identificadores:

- IP
- MAC
- ENI
- instance ID
- node UID
- hostname
- interface
- cloud metadata
- cgroup
- namespace
- temporal information

---

# 14. Grafo temporal

O grafo não pode assumir que:

IP = entidade permanente.

Exemplo:

```
Hoje:
10.10.1.5 = Pod A

Amanhã:
10.10.1.5 = Pod B
```

Portanto entidades e relações devem possuir validade temporal:

```
valid_from
valid_until
```

Isso é particularmente importante para:

- Kubernetes
- autoscaling
- ephemeral workloads
- ENIs
- NAT
- load balancing

---

# 15. Evidence Model

Cada edge do grafo deve dizer como aquela relação foi descoberta.

Tipos possíveis:

OBSERVED

Exemplo:
eBPF observou a conexão.

INFERRED

Exemplo:
Route Table indica que o próximo hop deve ser o TGW.

ACTIVE_TEST

Exemplo:
TCP probe confirmou reachability.

LOG_CONFIRMED

Exemplo:
Flow Log ou Firewall Log confirmou o tráfego.

DISCOVERED

Exemplo:
Cloud/Kubernetes API confirmou relação administrativa/topológica.

Cada relação deve possuir:

- evidence source
- timestamp
- confidence
- provider
- relevant metadata

Exemplo:

```
EC2
  ───────────────►
Transit Gateway

evidence:
AWS Route Table

type:
INFERRED

confidence:
HIGH
```

---

# 16. Confidence Engine

O Peabiru nunca deve afirmar como fato algo que foi somente inferido.

Exemplo de output:

```
EC2 → TGW
INFERRED
Confidence: HIGH

TGW → Firewall
LOG_CONFIRMED
Confidence: VERIFIED

K8s Node → Pod
OBSERVED VIA eBPF
Confidence: VERIFIED
```

A confiança end-to-end pode ser calculada com base nas evidências disponíveis.

---

# 17. Active Probe Engine

Não executar binários externos como:

```
mtr
traceroute
ping
```

A implementação pode reproduzir internamente as funcionalidades necessárias.

Exemplo de TCP traceroute:

```
TTL=1
TCP SYN :443

TTL=2
TCP SYN :443

TTL=3
TCP SYN :443
```

Isso permite testar o mesmo protocolo/porta utilizado pela aplicação.

Também pode implementar:

- latency probes
- reachability
- TCP handshake
- UDP probes
- MTU/PMTUD diagnostics
- DNS diagnostics

---

# 18. Evitar arquitetura "programa chamando programa"

Não fazer:

```
Peabiru
 ├── exec tcpdump
 ├── exec mtr
 ├── exec traceroute
 ├── exec bpftool
 ├── exec kubectl
 └── exec aws
```

Isso pode ser aceitável para PoC, mas não para a arquitetura desejada.

Preferência:

um software próprio utilizando diretamente bibliotecas e APIs.

Exemplo:

```
Rust Process

 ├── eBPF loader
 ├── Ring Buffer consumer
 ├── Netlink
 ├── TCP/raw socket engine
 ├── Packet parser
 ├── Active Probe Engine
 ├── AWS SDK
 ├── Kubernetes client
 ├── Graph Engine
 ├── Correlation Engine
 └── OTLP exporter
```

Importante:

Não significa reimplementar bibliotecas maduras do zero.

A lógica do produto deve ser própria.

Protocolos e interfaces maduras podem ser utilizados através de bibliotecas.

---

# 19. Stack tecnológica proposta

Core/userspace:

Rust

Possíveis alternativas para eBPF:

Opção inicial conservadora:

```
eBPF:
C

Loader:
libbpf-rs

Portabilidade:
BPF CO-RE / BTF
```

Possibilidade futura:

```
Aya
+
eBPF também em Rust
```

Outras tecnologias:

```
Netlink
Raw sockets
AWS SDK for Rust
Kubernetes API/client
OTLP/OpenTelemetry
```

Go também é tecnicamente viável, especialmente através de cilium/ebpf, mas Rust parece combinar melhor com a natureza low-level do projeto.

Não tornar "100% Rust" uma exigência ideológica.

Priorizar:

- estabilidade
- segurança
- performance
- compatibilidade
- manutenção

---

# 20. OpenTelemetry

OTel deve fazer parte da arquitetura, mas NÃO como mecanismo central de descoberta.

Função:

```
telemetry transport
integration
export
correlation with application tracing
```

Fluxo:

```
eBPF
Discovery
Correlation
    ↓
Flow aggregation
    ↓
OTLP
    ↓
OpenTelemetry Collector
    ↓
Tempo / Jaeger / Datadog / etc.
```

OTel deve ser opcional.

O Peabiru deve funcionar sozinho.

---

# 21. Correlação com Application Tracing

Uma oportunidade importante:

Application trace:

```
trace_id abc123

Service A
   ↓
Service B
```

Network trace:

```
Host A
 ↓
Gateway
 ↓
Firewall
 ↓
Load Balancer
 ↓
Kubernetes
 ↓
Service B
```

Peabiru pode eventualmente relacionar os dois:

```
REQUEST abc123

Application latency:
82 ms

Network latency:
13 ms

Source kernel:
0.3 ms

Destination kernel:
0.4 ms

Possible bottleneck:
Application
```

Isso pode ser um grande diferencial.

---

# 22. Lua

Lua NÃO deve estar no datapath.

Não fazer:

```
packet
 ↓
eBPF
 ↓
Lua
```

Lua pode futuramente ser usada para extensibilidade e diagnostics.

Exemplo:

```lua
if flow.retransmissions > 5 then
    finding("High TCP retransmission")
end
```

Outro exemplo:

```lua
if source:is_private()
   and path:contains("internet_gateway")
   and destination:is_private() then

    finding("Possible traffic tromboning")
end
```

No MVP, Lua provavelmente não é necessária.

---

# 23. Diagnostic Engine

Além de mostrar o caminho, o Peabiru deve eventualmente explicar possíveis problemas.

Exemplos:

- retransmissions
- RTT elevado
- drops
- RST
- MTU
- asymmetric routing
- unexpected NAT
- public/private path anomaly
- traffic tromboning/hairpinning
- firewall denial
- NetworkPolicy denial
- Security Group/NACL issue
- route mismatch
- DNS issue
- unexpected endpoint
- cross-zone/cross-region path
- unnecessary hops

Exemplo:

```
Private EC2
   ↓
NAT
   ↓
Internet
   ↓
Public Load Balancer
   ↓
Private Kubernetes
```

Finding:

Possible traffic tromboning.

Potential consequences:

- additional latency
- additional cost
- unnecessary public exposure
- architectural complexity

---

# 24. Exemplo de experiência final

Comando:

```
peabiru trace \
    --from aws://account-a/ec2/i-01234 \
    --to k8s://production/payments \
    --port 443
```

Possível output:

```
PEABIRU TRACE #84

payments-client
EC2 i-01234
10.10.1.23

     │
     │ OBSERVED
     │ eBPF TCP connect
     ▼

ENI eni-0678

     │
     │ INFERRED
     │ Route Table
     ▼

Transit Gateway

     │
     │ LOG_CONFIRMED
     ▼

Inspection VPC

     ▼

Network Firewall

     ▼

Internal ALB

     │
     │ DISCOVERED
     ▼

Target Group

     ▼

EKS Node

     │
     │ OBSERVED
     │ eBPF
     ▼

Service payments

     │
     │ KUBERNETES DISCOVERY
     ▼

EndpointSlice

     ▼

Pod payments-7f8c9
10.40.2.17

     │
     │ OBSERVED
     ▼

nginx PID 823


Diagnostics:

TCP RTT:             11.8 ms
Retransmissions:     2
Kernel drops:        0
RST:                 0

Path confidence:     89%

Potential problem:

Retransmissions detected between source and destination.
```

---

# 25. Arquitetura inicial do repositório

```
peabiru/
│
├── core/
│   ├── model/
│   ├── flow/
│   ├── graph/
│   └── events/
│
├── correlation/
│   ├── entity_resolution/
│   ├── path/
│   ├── temporal/
│   └── confidence/
│
├── sensor/
│   ├── ebpf/
│   ├── netlink/
│   ├── socket/
│   └── packet/
│
├── probe/
│   ├── tcp/
│   ├── traceroute/
│   ├── mtu/
│   └── dns/
│
├── discovery/
│   ├── aws/
│   ├── kubernetes/
│   ├── linux/
│   ├── azure/
│   ├── gcp/
│   ├── magalu/
│   ├── vmware/
│   └── network/
│
├── normalization/
│
├── diagnostics/
│
├── telemetry/
│   └── otel/
│
├── storage/
│
├── api/
│
├── cli/
│
└── ui/
```

---

# 26. MVP recomendado

Não tentar resolver multicloud/on-prem completo inicialmente.

Primeira versão:

Linux + AWS + Kubernetes.

eBPF:

- TCP connect
- close
- RTT
- retransmission
- RST
- basic drops
- PID/process
- cgroup/container

AWS Discovery:

- Account
- EC2
- ENI
- VPC
- Subnet
- Route Table
- SG
- NACL
- TGW
- ALB/NLB
- Target Groups

Kubernetes Discovery:

- Nodes
- Pods
- Services
- EndpointSlices
- Ingress
- NetworkPolicies

Core:

- normalized graph
- temporal graph
- entity resolution
- path search
- evidence model
- confidence model

Output inicial:

- CLI
- JSON
- OTLP

---

# 27. Expansões posteriores

Depois adicionar:

AWS:
- Network Firewall
- PrivateLink
- VPC Endpoint
- API Gateway
- WAF
- VPN
- Direct Connect
- Route53/DNS

Linux/eBPF:
- conntrack
- NAT tracking
- TC
- XDP
- deeper drop analysis

Kubernetes:
- CNI awareness
- Cilium/Hubble
- Calico
- kube-proxy/IPVS
- eBPF kube-proxy replacements

Clouds:
- Azure
- GCP
- Magalu

On-prem:
- VMware
- OpenStack
- BGP
- SNMP
- NETCONF
- RESTCONF
- Cisco
- Juniper
- Arista
- Fortinet
- Palo Alto

---

# 28. Problemas difíceis que precisam ser estudados

Alguns desafios provavelmente definirão a viabilidade técnica do Peabiru:

1. Entity Resolution

Como determinar com segurança que duas entidades descobertas por fontes diferentes são a mesma entidade?

2. Temporal Graph

Como representar infraestrutura altamente dinâmica?

3. NAT Correlation

Como acompanhar conexões antes/depois de NAT?

4. Load Balancers

Uma conexão:

client → LB

e outra:

LB → backend

são TCP sessions diferentes.

Como correlacioná-las?

5. Encryption

Como correlacionar requests sem depender de payload quando existe TLS?

6. Multicloud boundaries

Como correlacionar caminhos atravessando domínios administrativos independentes?

7. On-prem discovery

Como descobrir switches, routers, firewalls e caminhos físicos sem depender de uma única API universal?

8. Scale

Como trabalhar com milhões de conexões sem gerar eventos/pacotes excessivos?

9. Security

Como garantir least privilege para:

- eBPF capabilities
- IAM
- Kubernetes RBAC
- cloud credentials
- packet capture
- sensitive metadata

10. Confidence

Como calcular matematicamente confiança de um caminho construído com evidências de diferentes qualidades?

---

# 29. Princípios arquiteturais

O projeto deve seguir estes princípios:

1. Graph-first architecture.
2. Cloud-agnostic core.
3. Providers específicos ficam fora do Core.
4. Discovery fornece fatos, não decide o caminho.
5. Sensors fornecem observações.
6. Correlation conecta fatos.
7. Trace Engine procura caminhos.
8. Diagnostic Engine interpreta os caminhos.
9. Evidence e Confidence são first-class citizens.
10. Agent deve ser pequeno.
11. Agentless deve continuar possível.
12. Não depender de shelling-out para tcpdump/mtr/kubectl/aws/bpftool.
13. Utilizar bibliotecas maduras quando apropriado.
14. Packet capture deve ser on-demand.
15. OpenTelemetry é integração, não dependência fundamental.
16. O sistema deve poder evoluir de AWS/Kubernetes para multicloud e on-prem sem reescrever o Core.

---

# 30. Definição resumida do Peabiru

Peabiru é uma plataforma de Network Path Intelligence que correlaciona telemetria real do datapath com topologia de infraestrutura para reconstruir, explicar e diagnosticar a jornada completa de uma conexão entre dois endpoints.

Ela combina:

```
eBPF
+
Linux
+
Cloud Discovery
+
Kubernetes Discovery
+
Active Probes
+
Normalized Global Graph
+
Entity Resolution
+
Correlation Engine
+
Evidence/Confidence Engine
+
Diagnostics
+
OpenTelemetry
```

para transformar:

```
10.10.1.15 → 10.40.8.22:443
```

em algo compreensível como:

```
Java process
 ↓
EC2
 ↓
ENI
 ↓
Route Table
 ↓
Transit Gateway
 ↓
Network Firewall
 ↓
Internal ALB
 ↓
Target Group
 ↓
Kubernetes Node
 ↓
Service
 ↓
EndpointSlice
 ↓
Pod
 ↓
nginx process
```

com evidências, latência, retransmissões, drops, anomalias e nível de confiança associados a cada trecho.

---

# Solicitação para a próxima LLM (pauta aberta)

A partir desta especificação conceitual, NÃO redesenhe imediatamente o projeto inteiro.

Primeiro faça uma análise crítica e identifique:

- quais partes são tecnicamente viáveis;
- quais partes são difíceis mas possíveis;
- quais premissas estão incorretas;
- onde já existem projetos/open source que podem ser reutilizados;
- onde estamos reinventando funcionalidades existentes;
- quais limitações do kernel/cloud tornam alguma ideia inviável;
- como resolver entity resolution;
- como modelar o grafo;
- como representar tempo;
- como correlacionar NAT e Load Balancers;
- como separar observed path de inferred path;
- quais interfaces Rust devem existir;
- qual banco/modelo de armazenamento utilizar;
- qual arquitetura de agentes/control plane seria mais adequada;
- como suportar multicloud sem acoplar o Core aos providers;
- qual seria o menor MVP capaz de provar a hipótese principal.

Depois dessa crítica, proponha:

1. arquitetura técnica v1;
2. modelo de dados;
3. interfaces/traits principais;
4. estrutura de crates Rust;
5. arquitetura eBPF;
6. arquitetura do Control Plane;
7. protocolo Agent → Control Plane;
8. modelo do Global Graph;
9. algoritmo inicial de Correlation;
10. algoritmo inicial de Path Finding;
11. modelo de Evidence/Confidence;
12. roadmap MVP → produção;
13. riscos técnicos;
14. possíveis projetos open source para reutilização;
15. PoCs necessárias antes de iniciar a implementação completa.

Não assuma que todas as decisões deste documento estão corretas. Questione-as tecnicamente e proponha alternativas melhores quando houver.
