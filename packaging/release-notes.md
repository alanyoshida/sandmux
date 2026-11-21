sandmux 0.1.31, antes da v1: o registro dos arquivos pelo kernel mostra as aberturas de leitura e escrita como `rw`.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Correção

- **`rw` no registro pelo kernel:** numa aba com os [arquivos pelo kernel](https://github.com/alanyoshida/sandmux/blob/main/docs/files.md#pelo-kernel), uma abertura de leitura e escrita aparecia como `w`, e a mesma abertura numa aba pelo supervisor aparece como `rw`. Agora o registro diz `r`, `w` ou `rw` nos dois casos, também numa abertura bloqueada. Ler um arquivo e depois abri-lo para leitura e escrita dá as duas linhas.

Esta versão muda o auxiliar root. Depois de atualizar, atualize o auxiliar (`sandmux setup`); até lá, uma abertura de leitura e escrita continua aparecendo como `w`. O protocolo entre o cliente e o daemon não muda.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
