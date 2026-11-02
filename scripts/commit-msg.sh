#!/bin/sh
set -eu

# With `git commit -v` the diff follows the scissors line; it is not part of the message.
msg=$(sed '/^# -* >8 -*$/,$d' "$1")
header=$(printf '%s\n' "$msg" | grep -v '^#' | head -n 1)

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

if printf '%s\n' "$msg" | grep -qiE 'co-authored-by:.*(claude|anthropic)|generated (with|by) .*claude|noreply@anthropic\.com'; then
  echo "commit-msg: AI attribution is not allowed" >&2
  exit 1
fi

# Lines starting with '#' are checked too: `git commit -m` keeps them in the message.
if printf '%s\n' "$msg" | LC_ALL=C grep -qF "$(printf '\342\200\224')"; then
  echo "commit-msg: the em dash (U+2014) is not used in this repository" >&2
  exit 1
fi
