sandmux 0.1.36, antes da v1: corrige a recusa rápida da 0.1.35 durante a troca do auxiliar root.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Correção

- **Troca do auxiliar root:** a 0.1.35 fez o lançamento desistir em 1 s quando o socket do [auxiliar root](https://github.com/alanyoshida/sandmux/blob/main/docs/helper.md) não existe, num kernel com o Landlock abaixo do ABI 6. Mas o prazo contava desde a primeira tentativa. Numa troca do auxiliar (uma atualização), o auxiliar velho fica alguns segundos sem responder e o socket some por um instante depois; um lançamento nesse instante era recusado, quando deveria esperar o auxiliar novo. Agora o prazo de 1 s conta só a falta seguida do socket.

Esta versão muda só o lançador (`sandmux-run`). O protocolo e o auxiliar root não mudam.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
