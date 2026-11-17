#!/bin/sh
# Testes que exigem root: o confinamento do Docker privado por BPF LSM.
#
#   sudo scripts/check-root.sh [filtro de teste]
#
# Só o auxiliar (`target/debug/sandmux-helper`) roda como root, num socket
# temporário. Os testes (`crates/sandmux/tests/docker_ebpf.rs`) rodam como o
# usuário que chamou o sudo, com o Docker sem root dele. Um arquivo
# `<socket>.restart` criado pelos testes reinicia o auxiliar.
#
# Atenção: o script roda binários compilados pelo usuário. Liberá-lo no
# sudoers sem senha dá root a quem roda como esse usuário.
set -eu

[ "$(id -u)" -eq 0 ] || { echo "check-root: rode com sudo" >&2; exit 2; }
uid=${SUDO_UID:-}
if [ -z "$uid" ] || [ "$uid" = 0 ]; then
    echo "check-root: rode pelo sudo, a partir do seu usuário" >&2
    exit 2
fi
user=$(getent passwd "$uid" | cut -d: -f1)
home=$(getent passwd "$uid" | cut -d: -f6)
gid=$(getent passwd "$uid" | cut -d: -f4)

cd "$(dirname "$0")/.."
root=$(pwd)
helper="$root/target/debug/sandmux-helper"

as_user() {
    setpriv --reuid="$uid" --regid="$gid" --init-groups -- env -i \
        HOME="$home" USER="$user" LOGNAME="$user" \
        PATH="$home/.cargo/bin:/usr/local/bin:/usr/bin:/bin" \
        XDG_RUNTIME_DIR="/run/user/$uid" \
        DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" \
        CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-16}" \
        LANG="${LANG:-C.UTF-8}" \
        "$@"
}

echo "== compilando como $user"
as_user cargo build -q -p sandmux --bin sandmux-helper
[ -x "$helper" ] || { echo "check-root: falta $helper" >&2; exit 2; }

tmp=$(mktemp -d /run/sandmux-check-root.XXXXXX)
chown "$uid" "$tmp"
chmod 0755 "$tmp"
sock="$tmp/helper.sock"

# O auxiliar, reiniciado quando os testes pedem, até o fim.
(
    while :; do
        "$helper" --socket "$sock" 2>>"$tmp/helper.log" &
        h=$!
        while kill -0 "$h" 2>/dev/null && [ ! -e "$sock.restart" ] && [ ! -e "$tmp/fim" ]; do
            sleep 0.2
        done
        kill "$h" 2>/dev/null || true
        wait "$h" 2>/dev/null || true
        rm -f "$sock.restart"
        [ -e "$tmp/fim" ] && exit 0
        sleep 1
    done
) &
sup=$!
fim() {
    touch "$tmp/fim"
    wait "$sup" 2>/dev/null || true
    if [ -s "$tmp/helper.log" ]; then
        echo "== log do auxiliar"
        cat "$tmp/helper.log"
    fi
    rm -rf "$tmp"
}
trap fim EXIT INT TERM

i=0
while [ ! -S "$sock" ]; do
    i=$((i + 1))
    [ "$i" -lt 50 ] || { echo "check-root: o auxiliar não subiu" >&2; exit 1; }
    sleep 0.1
done

echo "== testes com o auxiliar root ($sock)"
as_user env SANDMUX_HELPER_SOCKET="$sock" \
    cargo test -q -p sandmux --test docker_ebpf -- --test-threads=1 "$@"
echo "ok"
