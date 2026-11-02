#!/usr/bin/env bash
# Sonda os dois alvos de dentro do pod cliente: o banco do cluster (svc) e o
# "RDS" (rds). Uma linha por alvo com o estado de cada camada: dns, tcp, app.
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
require_lab

in_client bash -s -- "$PROBE_TIMEOUT" \
  svc "$SVC_TARGET" "$DB_PORT" "$(svc_cluster_ip)" \
  rds "$RDS_TARGET" "$DB_PORT" "$(container_ip "$RDS_CONTAINER")" \
  <"$LAB_DIR/probe-pod.sh"
