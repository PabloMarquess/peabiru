# Segurança

Para relatar uma vulnerabilidade, use o relato privado do GitHub: aba **Security**,
**Report a vulnerability**. Não abra uma issue pública.

## O que o Peabiru faz com privilégios

Os sensores carregam programas eBPF e precisam de `CAP_BPF` e `CAP_PERFMON` (na prática,
root). Os programas só observam: nenhum descarta, altera ou redireciona pacotes, e o
verificador do kernel recusa qualquer programa que tente acessar memória fora do permitido.

As sondas ativas entram no namespace de rede do alvo e abrem conexões de teste, como
`dig` ou `nc` fariam. Não enviam dados além do mínimo de cada protocolo.

O lab em `labs/kind` usa um kubeconfig próprio e nunca lê nem altera o `~/.kube/config`.
