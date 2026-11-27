#!/usr/bin/env bash
# Regenera os screenshots e as animações do site (website/static/img/) com o
# VHS, rodando o sandmux real nesta máquina. Tudo fica isolado numa pasta
# temporária: daemon próprio (sem a unidade systemd do usuário), configuração,
# dados e estado próprios, nenhum contato com o auxiliar root e nenhum segredo
# real. O CI nunca roda este script; as imagens são commitadas.
#
# Uso: scripts/site-screens.sh [roteiro.tape ...]
#      sem argumentos, roda todos os roteiros de website/tapes/.
# Rode sob o vigia de memória: o VHS sobe um navegador headless.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
command -v vhs >/dev/null || { echo "site-screens: precisa do vhs (github.com/charmbracelet/vhs)" >&2; exit 1; }

if [ "$#" -gt 0 ]; then
  tapes=()
  for t in "$@"; do tapes+=("$(realpath "$t")"); done
else
  # setup.tape é incluído pelos outros, não roda sozinho.
  tapes=()
  for t in "$root"/website/tapes/*.tape; do
    [ "$(basename "$t")" = setup.tape ] || tapes+=("$t")
  done
fi

CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-16} cargo build --release --manifest-path "$root/Cargo.toml" --bins
release="$root/target/release"

# Caminho fixo, porque ele aparece na barra da TUI. O mkdir falha se a pasta
# já existe, e ninguém troca a pasta por um link antes. Cada roteiro começa
# com uma pasta nova e uma cópia dos binários em $demo/bin: todo processo da
# demonstração (o daemon e os supervisores, que não herdam este ambiente) tem
# o executável ali, e as abas reais do usuário nunca têm.
demo=/tmp/sandmux-demo
bin="$demo/bin"
fresh_demo() {
  mkdir -m 700 "$demo" || { echo "site-screens: $demo já existe; apague-a e rode de novo" >&2; exit 1; }
  mkdir -m 700 "$demo/run" "$demo/data" "$demo/config" "$demo/state" "$bin"
  local b
  for b in sandmux sandmuxd sandmux-run sandmux-chat sandmux-db-mcp; do
    cp "$release/$b" "$bin/"
  done
}

export XDG_RUNTIME_DIR="$demo/run"
export XDG_DATA_HOME="$demo/data"
export XDG_CONFIG_HOME="$demo/config"
export XDG_STATE_HOME="$demo/state"
export SANDMUX_DAEMON_START=direct
export SANDMUX_HELPER_SOCKET="$demo/sem-auxiliar.sock"
export SANDMUX_DEMO="$demo"
export PATH="$bin:$PATH"

demo_pids() {
  local p exe
  for p in /proc/[0-9]*; do
    exe=$(readlink "$p/exe" 2>/dev/null) || continue
    case "$exe" in "$bin"/*) echo "${p#/proc/}" ;; esac
  done
}

cleanup() {
  "$bin/sandmux" daemon stop >/dev/null 2>&1 || true
  local i left
  for i in 1 2 3 4 5; do
    left=$(demo_pids)
    [ -z "$left" ] && break
    sleep 1
  done
  if [ -n "$left" ]; then
    echo "site-screens: sobraram processos da demonstração: $left; encerrando" >&2
    # shellcheck disable=SC2086
    kill $left 2>/dev/null || true
    sleep 2
    left=$(demo_pids)
    # shellcheck disable=SC2086
    [ -z "$left" ] || kill -9 $left 2>/dev/null || true
    sleep 1
  fi
  rm -rf "$demo"
}

cd "$root/website/tapes"
for t in "${tapes[@]}"; do
  echo "site-screens: $(basename "$t")"
  fresh_demo
  trap cleanup EXIT
  vhs "$t"
  cleanup
  trap - EXIT
done
