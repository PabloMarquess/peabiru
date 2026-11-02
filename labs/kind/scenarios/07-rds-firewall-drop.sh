# shellcheck shell=bash disable=SC2034  # as variáveis são lidas pelo lib.sh (load_scenario)
TITLE="Firewall do RDS descarta a porta do banco"
LAYER="Fora do cluster (lado AWS)"
AWS_ANALOGY="Security Group do RDS sem regra de entrada para a origem"
EXPECT_SVC="dns=ok tcp=ok app=ok"
EXPECT_RDS="dns=ok tcp=timeout app=-"
PEABIRU_SHOULD="O SYN saiu do nó (depois do SNAT) e nada foi descartado dentro do cluster. O bloqueio está depois do nó. Na AWS, a prova viria da configuração do SG e do Flow Log com REJECT. Mesmo timeout do 06, causa diferente."

revert() { in_rds_netns nft delete table inet peabiru_lab 2>/dev/null || true; }

inject() {
  revert
  in_rds_netns nft -f - <<EOF
table inet peabiru_lab {
  chain input {
    type filter hook input priority 0; policy accept;
    tcp dport $DB_PORT drop
  }
}
EOF
}
