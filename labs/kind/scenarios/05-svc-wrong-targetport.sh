# shellcheck shell=bash disable=SC2034  # as variáveis são lidas pelo lib.sh (load_scenario)
TITLE="Service aponta para a porta errada do pod"
LAYER="Kubernetes Service / pod de destino"
AWS_ANALOGY="targetPort errado no Service (o banco escuta em outra porta)"
EXPECT_SVC="dns=ok tcp=refused app=-"
EXPECT_RDS="dns=ok tcp=ok app=ok"
PEABIRU_SHOULD="O SYN foi traduzido para pod:5433, atravessou até o nó do banco e o kernel do pod respondeu RST porque nada escuta nessa porta. Mesmo 'connection refused' do 04 e do 08, causa diferente."

patch_target_port() {
  k -n "$DB_NS" patch svc postgres --type json \
    -p "[{\"op\": \"replace\", \"path\": \"/spec/ports/0/targetPort\", \"value\": $1}]" >/dev/null
}

inject() { patch_target_port 5433; }
revert() { patch_target_port "$DB_PORT"; }
