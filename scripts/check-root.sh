#!/bin/sh
# Testes que exigem root: o confinamento do Docker privado por BPF LSM.
#
#   sudo scripts/check-root.sh [--host] [filtro de teste]
#
# O padrão para estes testes é a VM (`scripts/vm-ubuntu-2404.sh check-root`):
# eles carregam programas BPF no kernel, e um erro num programa global derruba
# a máquina inteira (aconteceu duas vezes em 2026-11-20). Aqui, no host, o
# script avisa e pergunta antes; `--host` dispensa a pergunta.
#
# Só o auxiliar (`target/debug/sandmux-helper`) roda como root, num socket
# temporário, por `systemd-run` com as restrições da unidade do serviço
# (`packaging/systemd/sandmux-helper.service.in`). Os testes
# (`crates/sandmux/tests/docker_ebpf.rs`) rodam como o usuário que chamou o
# sudo, com o Docker sem root dele. Um arquivo
# `<socket>.restart` criado pelos testes reinicia o auxiliar.
#
# Atenção: o script roda binários compilados pelo usuário. Liberá-lo no
# sudoers sem senha dá root a quem roda como esse usuário.
set -eu

host=0
if [ "${1:-}" = "--host" ]; then
    host=1
    shift
fi
cat >&2 <<'AVISO'
check-root: estes testes carregam programas BPF no kernel DESTA máquina. Um
erro num programa global derruba o sistema inteiro. O padrão é rodá-los numa
VM: scripts/vm-ubuntu-2404.sh up && scripts/vm-ubuntu-2404.sh check-root
AVISO
if [ "$host" -ne 1 ]; then
    if [ ! -t 0 ]; then
        echo "check-root: sem terminal para confirmar; passe --host para rodar no host" >&2
        exit 2
    fi
    printf 'check-root: rodar no host mesmo assim? [s/N] ' >&2
    read -r resp
    case "$resp" in s|S|sim|y|Y|yes) ;; *) echo "check-root: cancelado" >&2; exit 1 ;; esac
fi

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

# Sobras de uma rodada interrompida: o auxiliar de teste e as pastas.
for u in $(systemctl list-units --no-legend --plain 'sandmux-helper-check-*' 2>/dev/null | awk '{print $1}'); do
    systemctl stop "$u" 2>/dev/null || true
done
rm -rf /run/sandmux-check-root.* /var/lib/sandmux-check-root.*

tmp=$(mktemp -d /run/sandmux-check-root.XXXXXX)
# Do root (o serviço não tem CAP_DAC_OVERRIDE), com o sticky bit: os testes
# criam ali o arquivo que pede o reinício do auxiliar.
chmod 1777 "$tmp"
sock="$tmp/helper.sock"
# O binário numa pasta do root, fora do HOME (a unidade tem ProtectHome=yes)
# e de /run (noexec) e /var/tmp (PrivateTmp).
bindir=$(mktemp -d /var/lib/sandmux-check-root.XXXXXX)
chmod 0755 "$bindir"
install -m 0755 "$helper" "$bindir/sandmux-helper"
unit="sandmux-helper-check-$$"

# As propriedades do [Service] da unidade instalada, menos o ExecStart: o
# auxiliar roda com as mesmas restrições do serviço real. O socket é criado
# pelo próprio auxiliar (--socket), e por isso a pasta dele fica gravável.
props=$(sed -n '/^\[Service\]/,/^\[/p' packaging/systemd/sandmux-helper.service.in \
    | grep -E '^[A-Za-z]+=' | grep -v '^ExecStart=')

start_helper() {
    rm -f "$sock"
    set --
    while IFS= read -r p; do
        [ -n "$p" ] && set -- "$@" -p "$p"
    done <<FIM
$props
FIM
    systemd-run --quiet --unit="$unit" --collect "$@" \
        -p ReadWritePaths="$tmp" \
        "$bindir/sandmux-helper" --socket "$sock"
}

stop_helper() {
    systemctl stop "$unit" 2>/dev/null || true
    # O --collect apaga a unidade quando ela para.
    i=0
    while systemctl is-active --quiet "$unit" 2>/dev/null && [ "$i" -lt 50 ]; do
        i=$((i + 1))
        sleep 0.1
    done
}

# O auxiliar, reiniciado quando os testes pedem, até o fim (ou até o script
# principal morrer sem passar pelo `trap`).
start_helper
main=$$
(
    while [ ! -e "$tmp/fim" ] && kill -0 "$main" 2>/dev/null; do
        if [ -e "$sock.restart" ]; then
            stop_helper
            rm -f "$sock.restart"
            sleep 1
            start_helper
        fi
        sleep 0.2
    done
    [ -e "$tmp/fim" ] || stop_helper
) &
sup=$!
fim() {
    touch "$tmp/fim"
    wait "$sup" 2>/dev/null || true
    journalctl --quiet --no-pager -u "$unit" 2>/dev/null | tail -n 20 >"$tmp/helper.log" || true
    stop_helper
    if [ -s "$tmp/helper.log" ]; then
        echo "== log do auxiliar"
        cat "$tmp/helper.log"
    fi
    rm -rf "$tmp" "$bindir"
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
