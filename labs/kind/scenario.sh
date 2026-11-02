#!/usr/bin/env bash
# Lista, mostra, injeta e reverte as falhas do lab.
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

usage() {
  cat <<EOF
uso: ./scenario.sh <comando> [cenário]

  list              todos os cenários, com a camada e a analogia na AWS
  show <id>         detalhes, sintoma esperado e o que o Peabiru deve concluir
  inject <id>       aplica a falha (aceita prefixo: ./scenario.sh inject 07)
  revert <id>       desfaz a falha
  revert-all        desfaz todas as falhas ativas
EOF
}

cmd_list() {
  local f
  printf '%-26s %-38s %s\n' CENÁRIO CAMADA "ANALOGIA NA AWS"
  for f in "$LAB_DIR"/scenarios/*.sh; do
    load_scenario "$(basename "$f" .sh)"
    printf '%-26s %-38s %s\n' "$ID" "$LAYER" "$AWS_ANALOGY"
  done
}

cmd_show() {
  load_scenario "$1"
  cat <<EOF
$ID: $TITLE
  camada:            $LAYER
  analogia na AWS:   $AWS_ANALOGY
  sonda esperada:    svc[$EXPECT_SVC]  rds[$EXPECT_RDS]
  Peabiru deve ver:  $PEABIRU_SHOULD
EOF
}

cmd_revert_all() {
  local id
  [[ -s $ACTIVE_FILE ]] || { log "nenhuma falha ativa"; return; }
  for id in $(tac "$ACTIVE_FILE"); do scenario_revert "$id"; done
}

case ${1:-} in
  list)       cmd_list ;;
  show)       [[ -n ${2:-} ]] || { usage; exit 2; }; cmd_show "$2" ;;
  inject)     [[ -n ${2:-} ]] || { usage; exit 2; }; require_lab; scenario_inject "$2"; log "agora: ./probe.sh" ;;
  revert)     [[ -n ${2:-} ]] || { usage; exit 2; }; require_lab; scenario_revert "$2" ;;
  revert-all) require_lab; cmd_revert_all ;;
  *)          usage; exit 2 ;;
esac
