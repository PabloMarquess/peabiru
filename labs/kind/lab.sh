#!/usr/bin/env bash
# Sobe, derruba e mostra o lab kind do Peabiru.
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

usage() {
  cat <<EOF
uso: ./lab.sh <comando>

  up       cria o cluster kind, o "RDS", o "Route 53" e as cargas de teste
  down     remove tudo o que o lab criou (cluster e containers $LAB_NAME-*)
  status   nós, pods, containers e falhas ativas
EOF
}

pull_images() {
  local img
  for img in "$PG_IMAGE" "$TOOLS_IMAGE" "$DNS_IMAGE"; do
    docker image inspect "$img" >/dev/null 2>&1 && continue
    log "baixando $img"
    docker pull -q "$img" >/dev/null
  done
}

create_cluster() {
  if cluster_exists; then
    log "cluster $LAB_NAME já existe"
    kind get kubeconfig --name "$LAB_NAME" >"$LAB_KUBECONFIG"
  else
    log "criando o cluster $LAB_NAME (1 control-plane + 2 workers)"
    local args=(--name "$LAB_NAME" --config "$LAB_DIR/kind-config.yaml" --kubeconfig "$LAB_KUBECONFIG" --wait 180s)
    [[ -n $KIND_NODE_IMAGE ]] && args+=(--image "$KIND_NODE_IMAGE")
    kind create cluster "${args[@]}"
  fi
  chmod 600 "$LAB_KUBECONFIG"
  k label node "$LAB_NAME-worker" peabiru.lab/role=app --overwrite >/dev/null
  k label node "$LAB_NAME-worker2" peabiru.lab/role=db --overwrite >/dev/null
  log "carregando as imagens nos nós"
  load_image "$LAB_NAME-worker" "$TOOLS_IMAGE"
  load_image "$LAB_NAME-worker2" "$PG_IMAGE"
}

# Substitui o `kind load docker-image`, que falha com o armazenamento de imagens
# do Docker 29 ao importar todas as plataformas. Aqui vai só a da máquina.
load_image() {  # nó, imagem
  docker exec "$1" crictl inspecti -q "$2" >/dev/null 2>&1 && return
  docker save --platform "linux/$(docker version -f '{{.Server.Arch}}')" "$2" |
    docker exec -i "$1" ctr --namespace=k8s.io images import --digests --snapshotter=overlayfs - >/dev/null
}

start_container() {  # nome, argumentos do docker run
  local name=$1
  shift
  case $(container_state "$name") in
    absent)  docker run -d --name "$name" --label peabiru.lab=1 "$@" >/dev/null ;;
    paused)  docker unpause "$name" >/dev/null ;;
    running) ;;
    *)       docker start "$name" >/dev/null ;;
  esac
}

# Fora do cluster, na rede docker "kind", fica o que na AWS seria gerenciado:
# o RDS, o firewall dele (SG/NACL) e o Route 53 com a zona privada.
start_aws_side() {
  log "subindo o lado AWS: RDS, firewall do RDS e Route 53"
  start_container "$RDS_CONTAINER" --network kind -e POSTGRES_HOST_AUTH_METHOD=trust "$PG_IMAGE"
  # O fw entra no netns do rds; recriado a cada up para não herdar regras nem netns antigo.
  docker rm -f "$RDS_FW_CONTAINER" >/dev/null 2>&1 || true
  docker run -d --name "$RDS_FW_CONTAINER" --label peabiru.lab=1 \
    --network "container:$RDS_CONTAINER" --cap-add NET_ADMIN \
    "$TOOLS_IMAGE" sleep 2147483647 >/dev/null

  mkdir -p "$STATE_DIR/r53"
  cat >"$STATE_DIR/r53/Corefile" <<EOF
$RDS_ZONE:53 {
    file /zones/$RDS_ZONE.zone {
        reload 1s
    }
    log
    errors
}
EOF
  chmod 644 "$STATE_DIR/r53/Corefile"
  write_default_zone
  start_container "$R53_CONTAINER" --network kind -v "$STATE_DIR/r53:/zones:ro" "$DNS_IMAGE" -conf /zones/Corefile
}

# O CoreDNS do cluster encaminha lab.internal para o r53, como o CoreDNS do
# EKS encaminha para o resolver da VPC.
wire_cluster_dns() {
  local r53_ip old new
  r53_ip=$(container_ip "$R53_CONTAINER")
  [[ -n $r53_ip ]] || die "$R53_CONTAINER sem IP na rede kind"
  old=$(k -n kube-system get configmap coredns -o jsonpath='{.data.Corefile}')
  new=$(sed '/# peabiru-lab:begin/,/# peabiru-lab:end/d' <<<"$old")
  new+="
# peabiru-lab:begin
$RDS_ZONE:53 {
    errors
    forward . $r53_ip
}
# peabiru-lab:end"
  [[ $new == "$old" ]] && return
  log "ligando o CoreDNS do cluster à zona $RDS_ZONE"
  k -n kube-system patch configmap coredns --type merge \
    -p "$(jq -n --arg c "$new" '{data: {Corefile: $c}}')" >/dev/null
  k -n kube-system rollout restart deploy/coredns >/dev/null
  k -n kube-system rollout status deploy/coredns --timeout 120s >/dev/null
}

render_manifests() {
  local f
  for f in "$LAB_DIR"/manifests/*.yaml; do
    sed -e "s|@APP_NS@|$APP_NS|g" -e "s|@DB_NS@|$DB_NS|g" -e "s|@DB_PORT@|$DB_PORT|g" \
      -e "s|@PG_IMAGE@|$PG_IMAGE|g" -e "s|@TOOLS_IMAGE@|$TOOLS_IMAGE|g" "$f"
    printf '\n---\n'
  done
}

deploy_workloads() {
  log "aplicando o banco (nó db) e o cliente (nó app)"
  render_manifests | k apply -f - >/dev/null
  k -n "$DB_NS" rollout status deploy/postgres --timeout 180s >/dev/null
  k -n "$APP_NS" rollout status deploy/client --timeout 180s >/dev/null
}

cmd_up() {
  require_tools kind docker kubectl jq
  mkdir -p "$STATE_DIR"
  pull_images
  create_cluster
  start_aws_side
  wire_cluster_dns
  deploy_workloads
  [[ -s $ACTIVE_FILE ]] && warn "falhas ainda ativas: $(paste -sd' ' "$ACTIVE_FILE") (./scenario.sh revert-all)"
  log "lab pronto. Linha de base:"
  "$LAB_DIR/probe.sh"
}

cmd_down() {
  require_tools kind docker
  if cluster_exists; then
    log "removendo o cluster $LAB_NAME"
    kind delete cluster --name "$LAB_NAME" --kubeconfig "$LAB_KUBECONFIG"
  fi
  local c
  for c in "$RDS_FW_CONTAINER" "$R53_CONTAINER" "$RDS_CONTAINER"; do
    if docker rm -f "$c" >/dev/null 2>&1; then log "removido $c"; fi
  done
  rm -rf "$STATE_DIR"
}

cmd_status() {
  require_lab
  k get nodes -L peabiru.lab/role
  echo
  k get pods -A -l 'app in (client,postgres)' -o wide
  echo
  local c
  printf '%-22s %-9s %s\n' CONTAINER ESTADO IP
  for c in "$RDS_CONTAINER" "$RDS_FW_CONTAINER" "$R53_CONTAINER"; do
    printf '%-22s %-9s %s\n' "$c" "$(container_state "$c")" "$(container_ip "$c")"
  done | sed "s|^\($RDS_FW_CONTAINER .*\) *\$|\1 (netns do $RDS_CONTAINER)|"
  echo
  if [[ -s $ACTIVE_FILE ]]; then echo "falhas ativas: $(paste -sd' ' "$ACTIVE_FILE")"; else echo "nenhuma falha ativa"; fi
}

case ${1:-} in
  up)     cmd_up ;;
  down)   cmd_down ;;
  status) cmd_status ;;
  *)      usage; exit 2 ;;
esac
