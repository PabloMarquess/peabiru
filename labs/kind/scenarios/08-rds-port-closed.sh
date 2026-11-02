# shellcheck shell=bash disable=SC2034  # as variáveis são lidas pelo lib.sh (load_scenario)
TITLE="RDS de pé, mas nada escuta na porta"
LAYER="Fora do cluster (destino)"
AWS_ANALOGY="Aplicação apontando para a porta errada do RDS, ou o banco sem escutar"
EXPECT_SVC="dns=ok tcp=ok app=ok"
EXPECT_RDS="dns=ok tcp=refused app=-"
PEABIRU_SHOULD="O SYN atravessou o cluster, saiu do nó e voltou RST de fora do cluster. Diferente do 05, o RST não veio de um pod: veio do destino externo. Diferente do 04, o pacote saiu do nó."

revert() { in_rds_netns nft delete table inet peabiru_lab 2>/dev/null || true; }

inject() {
  revert
  in_rds_netns nft -f - <<EOF
table inet peabiru_lab {
  chain input {
    type filter hook input priority 0; policy accept;
    tcp dport $DB_PORT reject with tcp reset
  }
}
EOF
}
