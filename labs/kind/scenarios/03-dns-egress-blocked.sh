# shellcheck shell=bash disable=SC2034  # as variáveis são lidas pelo lib.sh (load_scenario)
TITLE="Política de saída libera o banco e esquece o DNS"
LAYER="Política de rede (saída do pod)"
AWS_ANALOGY="NetworkPolicy ou SG for Pods com saída só para 5432, sem a porta 53"
EXPECT_SVC="dns=timeout tcp=ok app=ok"
EXPECT_RDS="dns=timeout tcp=ok app=ok"
PEABIRU_SHOULD="A consulta DNS saiu do socket do pod e foi descartada no nó de origem pela política de rede, sem resposta nenhuma. TCP direto no IP do banco funciona. Mesmo sintoma de DNS do 02, causa diferente."

inject() {
  k apply -f - >/dev/null <<EOF
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: lab-egress-sem-dns
  namespace: $APP_NS
spec:
  podSelector:
    matchLabels:
      app: client
  policyTypes: [Egress]
  egress:
    - ports:
        - protocol: TCP
          port: $DB_PORT
EOF
}

revert() { k -n "$APP_NS" delete networkpolicy lab-egress-sem-dns --ignore-not-found >/dev/null; }
