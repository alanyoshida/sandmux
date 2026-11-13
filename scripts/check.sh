#!/bin/sh
# Verificação completa do workspace: formatação, lints e testes.
# Os testes que montam o sandbox precisam de user namespace sem privilégio
# (no Ubuntu, com o perfil AppArmor de packaging/apparmor instalado).
set -eu

cd "$(dirname "$0")/.."

echo "== cargo fmt --check"
cargo fmt --all -- --check

echo "== cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

echo "== cargo test"
cargo test --workspace

echo "ok"
