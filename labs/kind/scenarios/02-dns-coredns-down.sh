# shellcheck shell=bash disable=SC2034  # as variáveis são lidas pelo lib.sh (load_scenario)
TITLE="CoreDNS fora do ar"
LAYER="DNS (resolver do cluster)"
AWS_ANALOGY="CoreDNS do EKS com 0 réplicas (scale errado, nó drenado, OOM)"
EXPECT_SVC="dns=refused tcp=ok app=ok"
EXPECT_RDS="dns=refused tcp=ok app=ok"
PEABIRU_SHOULD="A consulta DNS foi recusada no próprio nó de origem: o Service kube-dns está sem endpoints e o kube-proxy devolveu ICMP port unreachable. Nada saiu do nó. Mesmo sintoma de DNS do 03, causa diferente."

inject() {
  k -n kube-system get deploy coredns -o jsonpath='{.spec.replicas}' >"$STATE_DIR/coredns.replicas"
  k -n kube-system scale deploy coredns --replicas 0 >/dev/null
  k -n kube-system wait --for=delete pod -l k8s-app=kube-dns --timeout 60s >/dev/null 2>&1 || true
}

revert() {
  local replicas=2
  [[ -f $STATE_DIR/coredns.replicas ]] && replicas=$(<"$STATE_DIR/coredns.replicas")
  k -n kube-system scale deploy coredns --replicas "$replicas" >/dev/null
  k -n kube-system rollout status deploy coredns --timeout 120s >/dev/null
}
