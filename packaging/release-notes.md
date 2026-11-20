sandmux 0.1.25, antes da v1: correções do lançador nos binários das releases.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Correções

- **Caminhos acrescentados ao sandbox** (`--ro`, `--rw` e a configuração): nos binários das releases, o lançador abria cada caminho das regras do Landlock para leitura.
  - Um caminho que o usuário não consegue listar ficava sem regra e sem acesso dentro do sandbox.
  - Um pipe com nome (FIFO) travava o lançamento.

  Agora o caminho é aberto sem ler o conteúdo, como nos binários de desenvolvimento.

Esta versão muda só o lançador. O protocolo entre o cliente e o daemon não muda (42), nem o auxiliar root.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
