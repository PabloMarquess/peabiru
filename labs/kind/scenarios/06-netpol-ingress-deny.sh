# shellcheck shell=bash disable=SC2034  # as variáveis são lidas pelo lib.sh (load_scenario)
TITLE="Política de rede bloqueia a entrada no banco"
LAYER="Política de rede (entrada no destino)"
AWS_ANALOGY="SG do pod do banco (SG for Pods) sem regra de entrada para a origem"
EXPECT_SVC="dns=ok tcp=timeout app=-"
EXPECT_RDS="dns=ok tcp=ok app=ok"
PEABIRU_SHOULD="O SYN saiu do nó de origem, chegou ao nó do banco e foi descartado ali pela política de rede, antes do pod. Na origem só aparecem retransmissões de SYN. Mesmo timeout do 07, causa diferente: aqui o descarte acontece dentro do cluster."

inject() {
  k apply -f - >/dev/null <<EOF
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: lab-deny-ingress
  namespace: $DB_NS
spec:
  podSelector: {}
  policyTypes: [Ingress]
EOF
}

revert() { k -n "$DB_NS" delete networkpolicy lab-deny-ingress --ignore-not-found >/dev/null; }
