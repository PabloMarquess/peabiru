#!/usr/bin/env bash
# Funções comuns do lab. Os outros scripts fazem `source` deste arquivo.
set -euo pipefail

LAB_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=lab.env
source "$LAB_DIR/lab.env"
STATE_DIR=$LAB_DIR/.state
LAB_KUBECONFIG=$STATE_DIR/kubeconfig
ACTIVE_FILE=$STATE_DIR/active

log()  { printf '\033[36m==>\033[0m %s\n' "$*" >&2; }
warn() { printf '\033[33m!! \033[0m %s\n' "$*" >&2; }
die()  { printf '\033[31mxx \033[0m %s\n' "$*" >&2; exit 1; }

# kubectl sempre com o kubeconfig e o contexto do lab. Nunca usa o
# ~/.kube/config nem o contexto atual, que pode ser um cluster de cliente.
k() { kubectl --kubeconfig "$LAB_KUBECONFIG" --context "$KIND_CONTEXT" "$@"; }

require_tools() {
  local t
  for t in "$@"; do command -v "$t" >/dev/null || die "falta '$t' no PATH"; done
}

cluster_exists() { kind get clusters 2>/dev/null | grep -qx "$LAB_NAME"; }

require_lab() {
  if ! cluster_exists || [[ ! -f $LAB_KUBECONFIG ]]; then die "o lab não está de pé: rode ./lab.sh up"; fi
}

container_ip() {
  docker inspect -f '{{with index .NetworkSettings.Networks "kind"}}{{.IPAddress}}{{end}}' "$1" 2>/dev/null || true
}

container_state() {
  local s
  s=$(docker inspect -f '{{.State.Status}}' "$1" 2>/dev/null) || true
  echo "${s:-absent}"
}

client_pod()   { k -n "$APP_NS" get pod -l app=client -o jsonpath='{.items[0].metadata.name}'; }
in_client()    { k -n "$APP_NS" exec -i "$(client_pod)" -- "$@"; }
in_rds_netns() { docker exec -i "$RDS_FW_CONTAINER" "$@"; }   # o fw divide o netns do rds

svc_cluster_ip() { k -n "$DB_NS" get svc postgres -o jsonpath='{.spec.clusterIP}'; }

# Zona privada servida pelo container r53, no papel do Route 53.
# $1 = registros além do SOA e do NS. O serial sobe a cada escrita para o
# plugin file do CoreDNS recarregar a zona.
write_zone() {
  local records=$1 serial prev=0
  mkdir -p "$STATE_DIR/r53"
  [[ -f $STATE_DIR/zone.serial ]] && prev=$(<"$STATE_DIR/zone.serial")
  serial=$(date +%s)
  (( serial > prev )) || serial=$((prev + 1))
  echo "$serial" >"$STATE_DIR/zone.serial"
  cat >"$STATE_DIR/r53/zone.tmp" <<EOF
\$ORIGIN $RDS_ZONE.
\$TTL 5
@  IN SOA ns.$RDS_ZONE. hostmaster.$RDS_ZONE. ( $serial 60 60 600 5 )
@  IN NS  ns.$RDS_ZONE.
$records
EOF
  chmod 644 "$STATE_DIR/r53/zone.tmp"
  mv "$STATE_DIR/r53/zone.tmp" "$STATE_DIR/r53/$RDS_ZONE.zone"
}

write_default_zone() { write_zone "orders-db IN A $(container_ip "$RDS_CONTAINER")"; }

# Cenários: scenarios/<id>.sh define TITLE, LAYER, AWS_ANALOGY, EXPECT_SVC,
# EXPECT_RDS, PEABIRU_SHOULD e as funções inject e revert. Aceita prefixo ("07").
scenario_file() {
  local matches=("$LAB_DIR"/scenarios/"$1"*.sh)
  [[ -e ${matches[0]} ]] || die "cenário desconhecido: $1 (veja ./scenario.sh list)"
  (( ${#matches[@]} == 1 )) || die "'$1' é ambíguo: ${matches[*]##*/}"
  printf '%s' "${matches[0]}"
}

load_scenario() {
  local f
  f=$(scenario_file "$1")
  unset -f inject revert
  # shellcheck disable=SC2034  # lidas por scenario.sh e verify.sh
  TITLE='' LAYER='' AWS_ANALOGY='' EXPECT_SVC='' EXPECT_RDS='' PEABIRU_SHOULD=''
  ID=$(basename "$f" .sh)
  # shellcheck source=/dev/null
  source "$f"
}

mark_active()   { grep -qx "$1" "$ACTIVE_FILE" 2>/dev/null || echo "$1" >>"$ACTIVE_FILE"; }
unmark_active() { [[ -f $ACTIVE_FILE ]] && sed -i "/^$1\$/d" "$ACTIVE_FILE"; return 0; }

scenario_inject() {
  load_scenario "$1"
  log "injetando $ID: $TITLE"
  inject
  mark_active "$ID"
  sleep "$SETTLE_SECONDS"
}

scenario_revert() {
  load_scenario "$1"
  log "revertendo $ID"
  revert
  unmark_active "$ID"
  sleep "$SETTLE_SECONDS"
}
