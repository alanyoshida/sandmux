#!/bin/sh
# Verificação completa do workspace: formatação, lints e testes.
# Os testes que montam o sandbox precisam de user namespace sem privilégio
# (no Ubuntu, com o perfil AppArmor de packaging/apparmor instalado).
# Com o cargo-nextest instalado, os testes rodam em paralelo entre todos os
# arquivos (.config/nextest.toml); sem ele, pelo cargo test, um arquivo por vez.
set -eu

cd "$(dirname "$0")/.."

echo "== cargo fmt --check"
cargo fmt --all -- --check

echo "== cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

if cargo nextest --version >/dev/null 2>&1; then
    echo "== cargo nextest run"
    cargo nextest run --workspace
else
    echo "== cargo test (sem o cargo-nextest; instale com: cargo install cargo-nextest --locked)"
    cargo test --workspace
fi

echo "ok"
