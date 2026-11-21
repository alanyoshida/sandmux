sandmux 0.1.29, antes da v1: o registro das aberturas de arquivo sai completo numa leitura em massa.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Correção

- **Registro completo com os [arquivos pelo kernel](https://github.com/alanyoshida/sandmux/blob/main/docs/files.md#pelo-kernel):** numa leitura em massa (um `grep -r`, um indexador, milhares de arquivos de uma vez), a fila do kernel podia encher. As aberturas a mais saíam do registro, e só a linha de descarte aparecia. Agora o registro sai completo:
  - o kernel não manda de novo a abertura do mesmo arquivo, no mesmo modo, em menos de 1 s;
  - o evento leva só os bytes do caminho;
  - o auxiliar root esvazia a fila numa thread própria;
  - quando a fila enche mesmo assim, o auxiliar congela a aba por alguns milissegundos, até ela esvaziar. Cada congelamento vai para o journal do auxiliar.
- **Mais rápido:** ler 13 mil arquivos da pasta de trabalho, numa VM do Ubuntu 24.04, custa 1,05x no kernel 6.8 e 1,14x no 7.0 (eram 1,14x e 1,39x), contra 2,7x e 3,0x do supervisor.

Esta versão muda só o auxiliar root. Depois de atualizar, atualize o auxiliar (`sandmux setup`). O protocolo entre o cliente e o daemon não muda.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
