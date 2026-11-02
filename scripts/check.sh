#!/bin/sh
set -eu

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo audit
sh scripts/gitleaks.sh
sh scripts/shellcheck.sh
sh scripts/workflows.sh
