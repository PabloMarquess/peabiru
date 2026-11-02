#!/bin/sh
set -eu

image=koalaman/shellcheck:v0.10.0@sha256:2097951f02e735b613f4a34de20c40f937a6c8f18ecb170612c88c34517221fb

files=$(git ls-files --cached --others --exclude-standard -- '*.sh' 'labs/kind/scenarios/*' '.githooks/*')
[ -n "$files" ] || exit 0
# shellcheck disable=SC2086
docker run --rm -v "$PWD:/repo:ro" -w /repo "$image" --external-sources $files
