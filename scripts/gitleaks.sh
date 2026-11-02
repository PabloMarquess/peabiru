#!/bin/sh
set -eu

image=ghcr.io/gitleaks/gitleaks:v8.30.1@sha256:c00b6bd0aeb3071cbcb79009cb16a60dd9e0a7c60e2be9ab65d25e6bc8abbb7f
config=$(cat "$(git rev-parse --show-toplevel)/.gitleaks.toml")

case "${1:-}" in
  --staged) git diff --cached --text --no-color ;;
  *) git log --all -p --text --no-color ;;
esac | docker run --rm -i -e GITLEAKS_CONFIG_TOML="$config" "$image" stdin --redact --no-banner
