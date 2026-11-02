#!/bin/sh
set -eu

LC_ALL=C
export LC_ALL
dash=$(printf '\342\200\224')

case "${1:-}" in
  --staged)
    diff=$(git diff --cached --no-ext-diff --no-textconv -U0 --no-color)
    hits=$(printf '%s\n' "$diff" | awk '/^diff --git /{h=0; next} /^@@/{h=1; next} h && /^\+/' | grep -aF "$dash" || true)
    ;;
  *)
    rc=0
    hits=$(git grep --untracked -nIF "$dash" -- . ':(exclude)Cargo.lock') || rc=$?
    [ "$rc" -le 1 ] || exit "$rc"
    ;;
esac

if [ -n "$hits" ]; then
  echo "no-em-dash: this repository does not use the em dash (U+2014); rewrite with a comma, colon or parentheses:" >&2
  printf '%s\n' "$hits" >&2
  exit 1
fi
