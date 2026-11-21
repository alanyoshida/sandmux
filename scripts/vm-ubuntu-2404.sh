#!/bin/bash
# Uma VM do Ubuntu 24.04 (QEMU/KVM) para testar o sandmux no kernel 6.8 e no
# HWE sem tocar no kernel do host. Os programas BPF novos só rodam aqui.
#
#   scripts/vm-ubuntu-2404.sh up          baixa a imagem (uma vez) e sobe a VM
#   scripts/vm-ubuntu-2404.sh ssh [cmd]   roda um comando na VM (ou abre um shell)
#   scripts/vm-ubuntu-2404.sh put <arq>   copia arquivos para ~ na VM
#   scripts/vm-ubuntu-2404.sh lsm-bpf     põe o bpf no lsm= do boot e reinicia
#   scripts/vm-ubuntu-2404.sh hwe         instala o kernel HWE e reinicia
#   scripts/vm-ubuntu-2404.sh kernel <ver> sobe no kernel instalado que começa
#                                         com <ver> (6.8, 7.0) e reinicia
#   scripts/vm-ubuntu-2404.sh docker      instala o Docker sem root na VM
#   scripts/vm-ubuntu-2404.sh check-root [filtro]
#                                         roda os testes com root na VM
#   scripts/vm-ubuntu-2404.sh reset       volta o disco ao estado da imagem
#   scripts/vm-ubuntu-2404.sh down        desliga a VM
#
# A imagem fica em ~/.cache/sandmux-vm/, conferida pelo SHA256SUMS do mesmo
# lugar (por https). O disco da VM é uma camada por cima da imagem. A VM tem
# 4 GB de RAM e 4 CPUs; um vigia desliga a VM se a memória disponível do host
# cair abaixo de 6 GB.
set -eu

DIR=${SANDMUX_VM_DIR:-$HOME/.cache/sandmux-vm}
URL=https://cloud-images.ubuntu.com/noble/current
IMG=noble-server-cloudimg-amd64.img
PORT=${SANDMUX_VM_PORT:-2224}
MEM=${SANDMUX_VM_MEM:-4096}
MIN_HOST_MB=6144

ORIG=$PWD
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$DIR"
cd "$DIR"

ssh_vm() {
    ssh -i "$DIR/id" -p "$PORT" -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ConnectTimeout=5 \
        ubuntu@127.0.0.1 "$@"
}

wait_ssh() {
    for _ in $(seq 1 120); do
        if ssh_vm true 2>/dev/null; then
            return 0
        fi
        if ! running; then
            echo "a VM parou" >&2
            return 1
        fi
        sleep 2
    done
    echo "a VM não respondeu pelo ssh" >&2
    return 1
}

running() {
    [ -f "$DIR/qemu.pid" ] && kill -0 "$(cat "$DIR/qemu.pid")" 2>/dev/null
}

fetch() {
    if [ ! -f "$IMG" ]; then
        echo "== baixando $URL/$IMG"
        curl -fL --retry 5 -o "$IMG.part" "$URL/$IMG"
        mv "$IMG.part" "$IMG"
    fi
    curl -fsSL --retry 5 -o SHA256SUMS "$URL/SHA256SUMS"
    grep " \*\?$IMG\$" SHA256SUMS | sed 's/ \*/  /' | sha256sum -c --quiet - || {
        echo "a soma da imagem não confere; apague $DIR/$IMG e rode de novo" >&2
        exit 1
    }
}

seed() {
    [ -f id ] || ssh-keygen -q -t ed25519 -N '' -f id
    mkdir -p seed
    cat >seed/meta-data <<EOF
instance-id: sandmux-vm
local-hostname: sandmux-vm
EOF
    cat >seed/user-data <<EOF
#cloud-config
users:
  - name: ubuntu
    sudo: ALL=(ALL) NOPASSWD:ALL
    shell: /bin/bash
    ssh_authorized_keys:
      - $(cat id.pub)
package_update: false
EOF
    genisoimage -quiet -output seed.iso -volid cidata -joliet -rock seed/user-data seed/meta-data
}

# Desliga a VM se a memória do host ficar baixa (roda enquanto a VM vive).
guard() {
    local pid=$1
    while kill -0 "$pid" 2>/dev/null; do
        a=$(awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo)
        if [ "$a" -lt "$MIN_HOST_MB" ]; then
            echo "VIGIA: memória do host em ${a} MB; desligando a VM" >>"$DIR/vigia.log"
            kill -9 "$pid"
        fi
        sleep 1
    done
}

up() {
    if running; then
        echo "a VM já está rodando"
        return 0
    fi
    fetch
    [ -f disk.qcow2 ] || qemu-img create -q -f qcow2 -F qcow2 -b "$IMG" disk.qcow2 20G
    [ -f seed.iso ] || seed
    qemu-system-x86_64 -enable-kvm -cpu host -smp 4 -m "$MEM" \
        -drive file=disk.qcow2,if=virtio \
        -drive file=seed.iso,if=virtio,media=cdrom \
        -netdev user,id=n0,hostfwd=tcp:127.0.0.1:"$PORT"-:22 \
        -device virtio-net-pci,netdev=n0 \
        -display none -serial file:"$DIR/console.log" \
        -daemonize -pidfile "$DIR/qemu.pid"
    (guard "$(cat "$DIR/qemu.pid")" </dev/null >/dev/null 2>&1 &)
    wait_ssh
    ssh_vm 'uname -r; cat /sys/kernel/security/lsm; echo'
}

reboot_vm() {
    ssh_vm 'sudo systemctl reboot' || true
    sleep 5
    wait_ssh
    ssh_vm 'uname -r; cat /sys/kernel/security/lsm; echo'
}

# Os testes com root (`docker_ebpf`) dentro da VM: os binários estáticos
# (musl) vão para o mesmo caminho do host, porque os testes acham os
# binários pelo caminho da compilação. O auxiliar roda como root na VM e é
# reiniciado quando um teste cria `<socket>.restart`, como no
# `scripts/check-root.sh`; os testes rodam como o usuário, sob o systemd de
# usuário (os tetos e o escopo de cada aba).
check_root() {
    if running; then
        echo "check-root: a VM já está rodando; compilar ao mesmo tempo aperta a memória" >&2
    fi
    local tgt=x86_64-unknown-linux-musl rel bins test
    rel="$ROOT/target/$tgt/release"
    echo "== compilando em musl"
    (cd "$ROOT" && CC_x86_64_unknown_linux_musl=${CC_x86_64_unknown_linux_musl:-gcc} \
        cargo build -q --release --target "$tgt" -p sandmux --bin sandmux-helper &&
        CC_x86_64_unknown_linux_musl=${CC_x86_64_unknown_linux_musl:-gcc} \
            cargo test -q --release --target "$tgt" -p sandmux --test docker_ebpf --no-run)
    test=$(ls -t "$rel"/deps/docker_ebpf-* | grep -v '\.d$' | head -n 1)
    bins="sandmux sandmuxd sandmux-run sandmux-probe sandmux-helper"
    rm -rf "$DIR/check"
    mkdir -p "$DIR/check"
    for b in $bins; do cp "$rel/$b" "$DIR/check/"; done
    cp "$test" "$DIR/check/docker_ebpf"
    tar -C "$DIR/check" -czf "$DIR/check.tgz" .
    # A VM sobe só depois da compilação: as duas juntas apertam a memória do
    # host (o vigia desliga a VM abaixo de 6 GB livres).
    running || up >/dev/null
    scp -q -i "$DIR/id" -P "$PORT" -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR "$DIR/check.tgz" ubuntu@127.0.0.1:
    echo "== testes com root na VM"
    # shellcheck disable=SC2029
    ssh_vm "set -e
        sudo mkdir -p '$rel' '$ROOT/target/$tgt/tmp' '$ROOT/target/tmp'
        sudo chown -R ubuntu: '$ROOT'
        tar -C '$rel' -xzf check.tgz
        # Os binários do teste ficam fora do perfil do AppArmor do sandmux-run.
        sudo sysctl -q kernel.apparmor_restrict_unprivileged_userns=0
        sudo loginctl enable-linger ubuntu
        d=/run/sandmux-vm-check; sock=\$d/helper.sock
        sudo rm -rf \$d; sudo mkdir -p \$d; sudo chmod 1777 \$d
        sudo sh -c \"while [ ! -e \$d/fim ]; do
            rm -f \$sock; '$rel/sandmux-helper' --socket \$sock >>\$d/helper.log 2>&1 & p=\\\$!
            while kill -0 \\\$p 2>/dev/null; do
                if [ -e \$sock.restart ] || [ -e \$d/fim ]; then rm -f \$sock.restart; kill \\\$p; wait \\\$p; fi
                sleep 0.2
            done
        done\" </dev/null >/dev/null 2>&1 &
        for i in \$(seq 1 100); do [ -S \$sock ] && break; sleep 0.1; done
        cd /tmp
        rc=0
        systemd-run --user --scope -q -- env LANG=pt_BR.UTF-8 SANDMUX_HELPER_SOCKET=\$sock \\
            timeout 1800 '$rel/docker_ebpf' --test-threads=1 $* || rc=\$?
        touch \$d/fim
        sleep 1
        echo '== log do auxiliar (fim)'; tail -n 5 \$d/helper.log
        sudo dmesg | grep -iE 'bug:|oops|call trace' | tail -n 3 || true
        exit \$rc"
}

case "${1:-}" in
up) up ;;
ssh)
    shift
    ssh_vm "$@"
    ;;
put)
    shift
    files=()
    for f in "$@"; do
        case "$f" in /*) files+=("$f") ;; *) files+=("$ORIG/$f") ;; esac
    done
    set -- "${files[@]}"
    scp -i "$DIR/id" -P "$PORT" -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR "$@" ubuntu@127.0.0.1:
    ;;
lsm-bpf)
    # A lista atual mais o bpf, no arquivo de configuração do GRUB.
    ssh_vm 'l=$(cat /sys/kernel/security/lsm); case ",$l," in *,bpf,*) exit 0;; esac
        echo "GRUB_CMDLINE_LINUX_DEFAULT=\"\$GRUB_CMDLINE_LINUX_DEFAULT lsm=$l,bpf\"" |
            sudo tee /etc/default/grub.d/99-sandmux-lsm.cfg >/dev/null
        sudo update-grub 2>/dev/null'
    reboot_vm
    ;;
hwe)
    ssh_vm 'sudo apt-get update -q && sudo DEBIAN_FRONTEND=noninteractive apt-get install -yq linux-generic-hwe-24.04'
    reboot_vm
    ;;
kernel)
    # O kernel do boot pelo GRUB_DEFAULT, entre os instalados: o 6.8 do 24.04
    # ou o HWE, para os testes nos dois.
    [ $# -ge 2 ] || { echo "uso: $0 kernel <versão>" >&2; exit 2; }
    # shellcheck disable=SC2029
    ssh_vm "set -e
        v=\$(ls /boot/vmlinuz-$2* 2>/dev/null | sort -V | tail -n 1 | sed 's,^/boot/vmlinuz-,,')
        [ -n \"\$v\" ] || { echo 'kernel $2 não instalado' >&2; exit 1; }
        echo \"GRUB_DEFAULT='Advanced options for Ubuntu>Ubuntu, with Linux \$v'\" |
            sudo tee /etc/default/grub.d/98-sandmux-kernel.cfg >/dev/null
        sudo update-grub 2>/dev/null"
    reboot_vm
    ;;
docker)
    # O Docker sem root, com o rootlesskit 2.0 ou mais novo (a rede `none`) e
    # o compose do repositório do Docker; o `busybox` dos testes.
    ssh_vm 'set -e
        sudo apt-get update -q >/dev/null
        sudo DEBIAN_FRONTEND=noninteractive apt-get install -yq docker.io uidmap busybox-static curl >/dev/null
        sudo install -m 0755 -d /etc/apt/keyrings
        sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
        echo "deb [arch=amd64 signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu noble stable" |
            sudo tee /etc/apt/sources.list.d/docker.list >/dev/null
        sudo apt-get update -q >/dev/null
        sudo DEBIAN_FRONTEND=noninteractive apt-get install -yq docker-ce-rootless-extras docker-compose-plugin >/dev/null
        sudo systemctl disable --now docker.service docker.socket >/dev/null 2>&1 || true
        sudo loginctl enable-linger ubuntu
        rootlesskit --version; docker compose version'
    ;;
check-root)
    shift
    check_root "$@"
    ;;
reset)
    if running; then
        echo "desligue a VM antes (down)" >&2
        exit 1
    fi
    rm -f disk.qcow2
    ;;
down)
    if running; then
        ssh_vm 'sudo systemctl poweroff' 2>/dev/null || true
        for _ in $(seq 1 30); do running || break; sleep 1; done
        running && kill "$(cat qemu.pid)"
    fi
    rm -f qemu.pid
    ;;
*)
    sed -n '2,18p' "$0"
    exit 2
    ;;
esac
