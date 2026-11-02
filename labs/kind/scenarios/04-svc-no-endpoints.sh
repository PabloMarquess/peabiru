# shellcheck shell=bash disable=SC2034  # as variáveis são lidas pelo lib.sh (load_scenario)
TITLE="Service do banco sem endpoints"
LAYER="Kubernetes Service"
AWS_ANALOGY="Deployment do banco com 0 réplicas, selector errado ou pods não prontos"
EXPECT_SVC="dns=ok tcp=refused app=-"
EXPECT_RDS="dns=ok tcp=ok app=ok"
PEABIRU_SHOULD="O SYN para o ClusterIP foi rejeitado no nó de origem pelo kube-proxy (ICMP port unreachable) porque o Service não tem endpoints. O pacote nem chegou ao nó do banco. Mesmo 'connection refused' do 05 e do 08, causa diferente."

inject() {
  k -n "$DB_NS" scale deploy postgres --replicas 0 >/dev/null
  k -n "$DB_NS" wait --for=delete pod -l app=postgres --timeout 90s >/dev/null 2>&1 || true
}

revert() {
  k -n "$DB_NS" scale deploy postgres --replicas 1 >/dev/null
  k -n "$DB_NS" rollout status deploy postgres --timeout 120s >/dev/null
}
