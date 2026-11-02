#!/bin/sh
set -eu

tag=${1:?usage: release.sh v<version>}
printf '%s\n' "$tag" | grep -qE '^v[0-9]+\.[0-9]+\.[0-9]+$' || { echo "release: '$tag' is not v<x.y.z>" >&2; exit 1; }
version=${tag#v}

declared=$(awk -F'"' '/^\[workspace\.package\]/ { s = 1; next } /^\[/ { s = 0 } s && $1 ~ /^version *= */ { print $2; exit }' Cargo.toml)
[ "$declared" = "$version" ] || { echo "release: Cargo.toml says $declared, the tag says $version" >&2; exit 1; }

notes=$(awk -v v="$version" '
  $0 ~ "^## " { if (found) exit; found = ($2 == v); next }
  found { print }
' CHANGELOG.md)
[ -n "$(printf '%s' "$notes" | tr -d '[:space:]')" ] || { echo "release: CHANGELOG.md has no entry for $version" >&2; exit 1; }

printf '%s\n' "$notes" | gh release create "$tag" --verify-tag --title "peabiru v$version" --notes-file -
