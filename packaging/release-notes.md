sandmux 0.1.30, antes da v1: o shell de entrada de uma aba pelo kernel vê os arquivos como o agente.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Correção

- **[Shell de entrada](https://github.com/alanyoshida/sandmux/blob/main/docs/sessions.md) pelo kernel:** numa aba com os [arquivos pelo kernel](https://github.com/alanyoshida/sandmux/blob/main/docs/files.md#pelo-kernel), o shell de `sandmux enter` e Ctrl+b `t` ainda passava cada abertura pelo supervisor. Agora ele segue o modo da aba: as aberturas são decididas e registradas pelo kernel, como as do agente, e o shell tem os mesmos filtros seccomp do comando. Numa aba pelo supervisor, nada muda.
- **Mais rápido:** ler 13 mil arquivos no shell de entrada, numa VM do Ubuntu 24.04, custa 1,22x no kernel 6.8 e 1,15x no 7.0 (eram 2,56x e 2,97x, pelo supervisor), o mesmo que no agente.

Esta versão muda só o lançador: o auxiliar root e o protocolo não mudam, e não é preciso rodar `sandmux setup`. Uma aba aberta antes da atualização continua com o supervisor no shell até ser reiniciada.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
