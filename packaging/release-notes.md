sandmux 0.1.13, antes da v1: a pasta atual do shell no modal de comando.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Pasta atual no modal de comando:** o modal de comando (Ctrl+b `g`) passa a mandar ao modelo a pasta em que o shell da aba está, e não mais a pasta em que a aba foi aberta. Depois de um `cd src/api`, vai `…/src/api`, e os comandos sugeridos acertam os caminhos relativos.
  - A linha `pasta` do modal mostra o que vai ao modelo.
  - Quando a pasta não pode ser lida (a aba parada, por exemplo), vai a pasta de trabalho da aba, como antes.

Detalhes em [docs/chat.md](https://github.com/alanyoshida/sandmux/blob/main/docs/chat.md#modal-de-comando).

Depois de atualizar, rode `sandmux daemon restart` (o protocolo passou a 33).

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
