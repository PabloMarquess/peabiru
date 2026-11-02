# shellcheck shell=bash disable=SC2034  # as variáveis são lidas pelo lib.sh (load_scenario)
TITLE="O registro do banco sumiu da zona privada"
LAYER="DNS (zona autoritativa)"
AWS_ANALOGY="Registro apagado ou renomeado na zona privada do Route 53"
EXPECT_SVC="dns=ok tcp=ok app=ok"
EXPECT_RDS="dns=nxdomain tcp=ok app=ok"
PEABIRU_SHOULD="A aplicação perguntou por orders-db.lab.internal, o CoreDNS encaminhou para o servidor da zona e ele respondeu NXDOMAIN. O caminho de rede até o IP do banco está bom: o problema é só o registro."

inject() { write_zone "orders-db-v2 IN A $(container_ip "$RDS_CONTAINER")"; }
revert() { write_default_zone; }
