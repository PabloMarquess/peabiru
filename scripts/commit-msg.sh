#!/bin/sh
set -eu

header=$(grep -v '^#' "$1" | head -n 1)

case "$header" in
  'Merge '* | 'Revert "'* | 'fixup! '* | 'squash! '*) ;;
  *)
    if ! printf '%s' "$header" | grep -qE '^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\([a-z0-9._/-]+\))?!?: [^ ].{0,90}$'; then
      echo "commit-msg: not a Conventional Commit: $header" >&2
      echo "expected: <type>(<scope>)?: <subject>, type in feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert" >&2
      exit 1
    fi
    ;;
esac

if grep -qiE 'co-authored-by:.*(claude|anthropic)|generated (with|by) .*claude|noreply@anthropic\.com' "$1"; then
  echo "commit-msg: AI attribution is not allowed" >&2
  exit 1
fi
