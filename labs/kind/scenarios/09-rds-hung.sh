# shellcheck shell=bash disable=SC2034  # as variáveis são lidas pelo lib.sh (load_scenario)
TITLE="Banco aceita a conexão, mas não responde"
LAYER="Aplicação (acima do TCP)"
AWS_ANALOGY="RDS travado ou sobrecarregado: o handshake fecha, o Postgres não responde"
EXPECT_SVC="dns=ok tcp=ok app=ok"
EXPECT_RDS="dns=ok tcp=ok app=timeout"
PEABIRU_SHOULD="A rede está inteira: handshake completo, RTT normal, os bytes do cliente foram confirmados (ACK) e nenhum byte voltou do servidor. O problema está acima do TCP, no processo do banco. Um nc diria que está tudo bem."

# O kernel do destino continua completando o handshake com o processo congelado.
inject() { docker pause "$RDS_CONTAINER" >/dev/null; }

revert() {
  [[ $(container_state "$RDS_CONTAINER") == paused ]] || return 0
  docker unpause "$RDS_CONTAINER" >/dev/null
}
