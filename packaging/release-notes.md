sandmux 0.1.35, antes da v1: num kernel antigo sem o auxiliar root, o lançamento é recusado na hora, sem esperar 30 s.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Correção

- **Recusa rápida sem o auxiliar root:** num kernel com o Landlock abaixo do ABI 6 (como o 6.8 do Ubuntu 24.04), o sandbox depende do [auxiliar root](https://github.com/alanyoshida/sandmux/blob/main/docs/helper.md). Sem ele instalado, o lançamento tentava falar com o auxiliar por 30 s antes de recusar e apontar o `sandmux doctor`. Agora, sem o socket do auxiliar, a recusa sai em 1 s. A espera de até 30 s continua valendo para a troca do auxiliar numa atualização.

Esta versão muda só o lançador (`sandmux-run`). O protocolo e o auxiliar root não mudam.

Para quem desenvolve: o `./scripts/check.sh` roda os testes em paralelo pelo [cargo-nextest](https://nexte.st), quando ele está instalado.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
