sandmux 0.1.24, antes da v1: o Ubuntu 24.04.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Ubuntu 24.04:** o sandmux roda no Ubuntu 24.04. O `sandmux doctor` mostra os dois caminhos, com os comandos:
  - **kernel HWE (recomendado):** `sudo apt install linux-generic-hwe-24.04` e reiniciar. Com ele, o sandmux roda como em qualquer kernel novo.
  - **kernel original (6.8):** o auxiliar root (`sandmux-helper`) completa por BPF LSM o que falta ao Landlock desse kernel: o bloqueio de sockets abstratos e de sinais para fora do sandbox, e de `ioctl` em dispositivos. É preciso ligar o BPF LSM no boot (o `doctor` mostra o comando) e rodar `sandmux setup`.
  - Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md#ubuntu-2404).
- **Docker privado no Ubuntu 24.04:** precisa do `rootlesskit` 2.0 ou mais novo, do pacote `docker-ce-rootless-extras` do repositório do Docker. O `doctor` avisa quando o `rootlesskit` é antigo.

## Correção

- **Docker confinado por eBPF:** desde a 0.1.20, com o auxiliar root ligado, o Docker privado não subia com os binários das releases. A abertura dos arquivos protegidos perdia uma opção nos binários estáticos (musl), e um arquivo que o usuário não lê, como o `/etc/gshadow`, parava o confinamento.

Esta versão muda o auxiliar root e o lançador. O protocolo entre o cliente e o daemon não muda (42). Depois de atualizar, rode `sandmux setup`, que reinicia o auxiliar, e `sandmux daemon restart`.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
