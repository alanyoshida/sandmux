#!/bin/sh
# Testa a distribuição de dist/ (scripts/dist.sh) em contêineres do Debian 13,
# do Fedora atual e do Arch: pacote nativo como root, install.sh sem
# privilégio como usuário comum, `sandmux doctor` e um sandbox simples. No
# Fedora, gera também o .rpm em dist/.
#
#   scripts/distro-test.sh [debian|fedora|arch ...]
#
# O kernel, o Landlock e o AppArmor são os da máquina: o teste prova os
# binários, os pacotes e o espaço de usuário de cada distro.
set -eu

cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
TAR="sandmux-$VERSION-x86_64-linux.tar.gz"
[ -f "dist/$TAR" ] || { echo "falta dist/$TAR: rode scripts/dist.sh" >&2; exit 1; }
DISTROS=${*:-debian fedora arch}

# O Docker bloqueia por padrão o user namespace e a montagem de /proc que o
# sandbox usa.
OPTS="--rm --memory 2g --pids-limit 4096
  --security-opt seccomp=unconfined --security-opt apparmor=unconfined
  --security-opt systempaths=unconfined"

# Passos comuns, como o usuário `u`: install.sh sem privilégio, doctor e um
# sandbox sem rede.
USER_STEPS="set -e
sh /dist/install.sh --no-sudo --no-setup --from /dist/$TAR
~/.local/bin/sandmux --version
~/.local/bin/sandmux doctor || true
mkdir -p ~/proj && cd ~/proj
SANDMUX_DAEMON_START=direct ~/.local/bin/sandmux run shell --network sem-rede -- -c 'echo sandbox-ok-\$((40+2)); cat /proc/self/status | grep -c ^Seccomp:.2'
"

run() {
    name=$1 image=$2 script=$3
    echo "======== $name ($image)"
    # shellcheck disable=SC2086
    docker run $OPTS -v "$PWD/dist:/dist" -v "$PWD/packaging:/packaging:ro" \
        -e USER_STEPS="$USER_STEPS" "$image" sh -c "$script"
}

for d in $DISTROS; do
    case "$d" in
    debian)
        run debian debian:13 '
set -e
dpkg -i /dist/sandmux_*_amd64.deb
ls -l /usr/bin/sandmux-run /etc/apparmor.d/sandmux-run
sandmux --version
useradd -m u
su u -c "$USER_STEPS"
dpkg -r sandmux >/dev/null && ! test -e /usr/bin/sandmux && echo removido-ok'
        ;;
    fedora)
        run fedora fedora:latest '
set -e
dnf -q -y install rpm-build >/dev/null
R=$(mktemp -d); mkdir -p $R/SOURCES; cp /dist/sandmux-*-x86_64-linux.tar.gz $R/SOURCES/
V=$(ls /dist/sandmux-*-x86_64-linux.tar.gz | head -n1 | sed "s|.*/sandmux-\(.*\)-x86_64-linux.tar.gz|\1|")
rpmbuild --quiet -bb --define "_topdir $R" --define "sandmux_version $V" /packaging/rpm/sandmux.spec
cp $R/RPMS/x86_64/sandmux-$V-1.x86_64.rpm /dist/
(cd /dist && sha256sum sandmux-$V-1.x86_64.rpm > sandmux-$V-1.x86_64.rpm.sha256)
chown "$(stat -c %u:%g /dist)" /dist/sandmux-$V-1.x86_64.rpm /dist/sandmux-$V-1.x86_64.rpm.sha256
dnf -q -y install /dist/sandmux-$V-1.x86_64.rpm >/dev/null
ls -l /usr/bin/sandmux-run /etc/apparmor.d/sandmux-run
sandmux --version
useradd -m u
su u -c "$USER_STEPS"
dnf -q -y remove sandmux >/dev/null && ! test -e /usr/bin/sandmux && echo removido-ok'
        ;;
    arch)
        run arch archlinux:latest '
set -e
sh /dist/install.sh --no-setup --from /dist/sandmux-*-x86_64-linux.tar.gz
ls -l /usr/local/bin/sandmux-run
sandmux --version
useradd -m u
mkdir -p /tmp/aur && cp /dist/aur/PKGBUILD /dist/aur/sandmux.install /tmp/aur/ && chown -R u /tmp/aur
su u -c "cd /tmp/aur && makepkg --printsrcinfo" > /tmp/aur/.SRCINFO.makepkg
a=$(grep -v "^$" /tmp/aur/.SRCINFO.makepkg); b=$(grep -v "^$" /dist/aur/.SRCINFO)
if [ "$a" = "$b" ]; then echo srcinfo-ok; else echo "SRCINFO diferente:"; echo "$a"; exit 1; fi
su u -c "$USER_STEPS"
sh /dist/install.sh --uninstall && ! test -e /usr/local/bin/sandmux && echo removido-ok'
        ;;
    *) echo "distro desconhecida: $d" >&2; exit 1 ;;
    esac
done
echo "======== ok"
