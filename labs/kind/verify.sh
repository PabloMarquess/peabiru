#!/usr/bin/env bash
# Prova o próprio lab: injeta cada falha, sonda, compara com o sintoma esperado
# e reverte. O resultado é o gabarito que o Peabiru vai ter que acertar depois.
# uso: ./verify.sh [id...]   (sem argumentos, roda todos os cenários)
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
require_lab

BASELINE="dns=ok tcp=ok app=ok"
state_of() { awk -v t="$1" '$1 == t { print $2, $3, $4 }' <<<"$2"; }
indent() { sed 's/^/      | /'; }

ids=("$@")
if (( ${#ids[@]} == 0 )); then
  for f in "$LAB_DIR"/scenarios/*.sh; do ids+=("$(basename "$f" .sh)"); done
fi

current=
trap '[[ -z $current ]] || { warn "interrompido: revertendo $current"; scenario_revert "$current" || true; }' EXIT

# Pré-voo: uma falha esquecida (inject sem revert) contamina a matriz inteira.
if [[ -s $ACTIVE_FILE ]]; then
  warn "falhas ativas antes de começar: $(paste -sd' ' "$ACTIVE_FILE"); revertendo"
  for id in $(tac "$ACTIVE_FILE"); do scenario_revert "$id"; done
fi
start=$("$LAB_DIR/probe.sh")
if [[ $(state_of svc "$start") != "$BASELINE" || $(state_of rds "$start") != "$BASELINE" ]]; then
  indent <<<"$start"
  die "o lab não está na linha de base antes de começar (veja ./lab.sh status)"
fi

pass=0 fail=0
for id in "${ids[@]}"; do
  load_scenario "$id"
  id=$ID
  current=$id
  scenario_inject "$id"
  out=$("$LAB_DIR/probe.sh")
  scenario_revert "$id"
  current=
  back=$("$LAB_DIR/probe.sh")

  load_scenario "$id"
  got_svc=$(state_of svc "$out")
  got_rds=$(state_of rds "$out")
  if [[ $got_svc == "$EXPECT_SVC" && $got_rds == "$EXPECT_RDS" ]]; then
    printf '\033[32mPASS\033[0m  %-26s svc[%s]  rds[%s]\n' "$id" "$got_svc" "$got_rds"
    pass=$((pass + 1))
  else
    printf '\033[31mFAIL\033[0m  %-26s\n      esperado svc[%s]  rds[%s]\n      obtido   svc[%s]  rds[%s]\n' \
      "$id" "$EXPECT_SVC" "$EXPECT_RDS" "$got_svc" "$got_rds"
    indent <<<"$out"
    fail=$((fail + 1))
  fi

  if [[ $(state_of svc "$back") != "$BASELINE" || $(state_of rds "$back") != "$BASELINE" ]]; then
    indent <<<"$back"
    die "$id não voltou à linha de base depois do revert; parei para não contaminar os próximos"
  fi
done

echo
echo "$pass de $((pass + fail)) cenários conferem com o esperado"
(( fail == 0 ))
