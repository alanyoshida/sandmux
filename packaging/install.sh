#!/bin/sh
# Instalador do sandmux.
#
#   curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
#
# Pede sudo logo de início para instalar em /usr/local/bin e o perfil do
# AppArmor do lançador. Sem sudo (recusado, indisponível ou --no-sudo),
# instala em ~/.local/bin. Depois roda `sandmux setup` como o usuário.
#
# Opções:
#   --no-sudo          não usa sudo; instala em ~/.local/bin
#   --prefix DIR       pasta dos binários (no lugar do padrão)
#   --from ORIGEM      pasta extraída ou tarball local, no lugar do download
#   --version vX.Y.Z   versão da release (padrão: a mais nova)
#   --no-setup         não roda `sandmux setup`
#   --uninstall        remove os binários, o perfil, a unidade do daemon e as
#                      do auxiliar root
set -eu

REPO="alanyoshida/sandmux"
BINS="sandmux sandmuxd sandmux-run sandmux-chat sandmux-db-mcp sandmux-helper"
UNITS="/etc/systemd/system/sandmux-helper.socket /etc/systemd/system/sandmux-helper.service"
PROFILE=/etc/apparmor.d/sandmux-run

say() { printf 'sandmux: %s\n' "$*" >&2; }
die() { say "erro: $*"; exit 1; }

no_sudo=0
prefix=""
from=""
version=""
setup=1
uninstall=0
while [ $# -gt 0 ]; do
    case "$1" in
        --no-sudo) no_sudo=1 ;;
        --prefix) [ $# -ge 2 ] || die "falta a pasta de --prefix"; prefix=$2; shift ;;
        --from) [ $# -ge 2 ] || die "falta a origem de --from"; from=$2; shift ;;
        --version) [ $# -ge 2 ] || die "falta a versão de --version"; version=$2; shift ;;
        --no-setup) setup=0 ;;
        --uninstall) uninstall=1 ;;
        -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "opção desconhecida: $1" ;;
    esac
    shift
done

[ "$(uname -s)" = Linux ] || die "o sandmux só roda no Linux"
case "$(uname -m)" in
    x86_64|amd64) ;;
    *) die "arquitetura $(uname -m) sem binário publicado; só x86_64 por enquanto (compile com cargo install)" ;;
esac

# Privilégio: root direto; senão sudo, pedido logo de início.
SUDO=""
priv=0
if [ "$(id -u)" -eq 0 ]; then
    priv=1
elif [ "$no_sudo" -eq 0 ] && command -v sudo >/dev/null 2>&1; then
    say "pedindo sudo para instalar em /usr/local/bin e o perfil do AppArmor (--no-sudo instala só no seu HOME)"
    if sudo -v; then
        SUDO=sudo
        priv=1
    else
        say "sem sudo: instalando em ~/.local/bin"
    fi
fi
if [ -z "$prefix" ]; then
    if [ "$priv" -eq 1 ]; then prefix=/usr/local/bin; else prefix="$HOME/.local/bin"; fi
fi
# Pasta do usuário: sem sudo, mesmo com privilégio.
as_dest() {
    case "$prefix" in
        "$HOME"/*) "$@" ;;
        *) $SUDO "$@" ;;
    esac
}

if [ "$uninstall" -eq 1 ]; then
    if [ -x "$prefix/sandmux" ] && [ "$(id -u)" -ne 0 ]; then
        "$prefix/sandmux" daemon disable >/dev/null 2>&1 || true
    fi
    # O auxiliar root para antes de o binário sumir.
    if [ -e /etc/systemd/system/sandmux-helper.socket ]; then
        if [ "$priv" -eq 1 ]; then
            $SUDO systemctl disable --now sandmux-helper.socket 2>/dev/null || true
            $SUDO systemctl stop sandmux-helper.service 2>/dev/null || true
            $SUDO rm -f $UNITS
            $SUDO systemctl daemon-reload 2>/dev/null || true
        else
            say "as unidades do sandmux-helper em /etc/systemd/system ficam: remova como root"
        fi
    fi
    for b in $BINS; do
        [ -e "$prefix/$b" ] && as_dest rm -f "$prefix/$b"
    done
    if [ -e "$PROFILE" ]; then
        if [ "$priv" -eq 1 ]; then
            command -v apparmor_parser >/dev/null 2>&1 && $SUDO apparmor_parser -R "$PROFILE" 2>/dev/null || true
            $SUDO rm -f "$PROFILE"
        else
            say "o perfil $PROFILE fica: remova como root"
        fi
    fi
    say "removido de $prefix. A configuração (~/.config/sandmux), os dados (~/.local/share/sandmux) e o estado (~/.local/state/sandmux) ficam; apague-os se quiser."
    exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

# Origem: --from, o tarball extraído ao lado do script, ou a release.
here=$(cd "$(dirname "$0")" 2>/dev/null && pwd || echo "")
src=""
if [ -n "$from" ]; then
    if [ -d "$from" ]; then
        src=$from
    elif [ -f "$from" ]; then
        tar -xzf "$from" -C "$tmp" || die "não foi possível extrair $from"
        src=$(find "$tmp" -maxdepth 2 -type d -name bin -exec dirname {} \; | head -n 1)
    else
        die "$from não existe"
    fi
elif [ -n "$here" ] && [ -x "$here/bin/sandmux" ]; then
    src=$here
else
    name=sandmux-x86_64-linux.tar.gz
    if [ -n "$version" ]; then
        name="sandmux-${version#v}-x86_64-linux.tar.gz"
        url="https://github.com/$REPO/releases/download/$version/$name"
    else
        url="https://github.com/$REPO/releases/latest/download/$name"
    fi
    say "baixando $url"
    if command -v curl >/dev/null 2>&1; then
        get() { curl -fsSL -o "$2" "$1"; }
    elif command -v wget >/dev/null 2>&1; then
        get() { wget -qO "$2" "$1"; }
    else
        die "falta curl ou wget"
    fi
    get "$url" "$tmp/$name" || die "download falhou: $url"
    get "$url.sha256" "$tmp/$name.sha256" || die "download falhou: $url.sha256"
    want=$(cut -d' ' -f1 <"$tmp/$name.sha256")
    have=$(sha256sum "$tmp/$name" | cut -d' ' -f1)
    [ "$want" = "$have" ] || die "SHA-256 não confere ($have, esperado $want); nada foi instalado"
    tar -xzf "$tmp/$name" -C "$tmp" || die "não foi possível extrair $name"
    src=$(find "$tmp" -maxdepth 2 -type d -name bin -exec dirname {} \; | head -n 1)
fi
[ -n "$src" ] && [ -x "$src/bin/sandmux" ] || die "não achei bin/sandmux na origem"

as_dest mkdir -p "$prefix"
for b in $BINS; do
    if [ ! -f "$src/bin/$b" ]; then
        # Releases anteriores à 0.1.20 não têm o auxiliar root.
        [ "$b" = sandmux-helper ] && continue
        die "falta $b em $src/bin"
    fi
    as_dest install -m 0755 "$src/bin/$b" "$prefix/$b"
done
say "binários instalados em $prefix"
case ":$PATH:" in
    *":$prefix:"*) ;;
    *) say "aviso: $prefix não está no PATH; acrescente-o ao seu shell" ;;
esac

[ "$setup" -eq 1 ] || exit 0
# O setup roda como o usuário; o perfil, com o sudo que já foi aceito.
args=""
[ "$priv" -eq 1 ] || args="--no-sudo"
if [ "$(id -u)" -eq 0 ]; then
    args="--no-daemon"
fi
# A pergunta do sudo precisa do terminal, mesmo com o script vindo por pipe.
# shellcheck disable=SC2086
if (exec </dev/tty) 2>/dev/null; then
    "$prefix/sandmux" setup $args </dev/tty || true
else
    "$prefix/sandmux" setup $args </dev/null || true
fi
