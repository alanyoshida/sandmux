sandmux 0.1.26, antes da v1: o Claude Code e o bubblewrap dentro do sandbox.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **bubblewrap dentro do sandbox:** o `bwrap` de uma aba é um substituto do sandmux. Ele executa o comando sem montar nada, já dentro do confinamento da aba, e aplica do pedido as pastas liberadas (um Landlock a mais só naquele comando), a rede e os processos próprios e o filtro seccomp. O sandbox do bash do Claude Code (`/sandbox`) funciona dentro do sandmux. Nenhuma aba perde proteção. Detalhes em [docs/claude.md](https://github.com/alanyoshida/sandmux/blob/main/docs/claude.md#sandbox-do-bash).

## Correção

- **Claude Code numa aba:** ele não subia (`EINVAL` ao abrir a pasta temporária dele). O supervisor de arquivos recusava a abertura de um diretório que trazia um `mode` sem `O_CREAT`, como o Bun faz em toda abertura.

Esta versão muda o lançador. O protocolo entre o cliente e o daemon não muda (42), nem o auxiliar root.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
