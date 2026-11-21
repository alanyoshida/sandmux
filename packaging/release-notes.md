sandmux 0.1.27, antes da v1: os arquivos das abas pelo kernel.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Arquivos das abas pelo kernel:** com o [auxiliar root](https://github.com/alanyoshida/sandmux/blob/main/docs/helper.md) e o BPF LSM, o kernel decide e registra as aberturas de arquivo de cada aba, no lugar do supervisor. Os segredos da pasta de trabalho dão `ENOENT` e o `file-deny` dá `EACCES`, como antes, agora por marcas no inode. As aberturas entram no registro pela fila do kernel. Ler muitos arquivos fica bem mais rápido: numa VM do Ubuntu 24.04, ler 13 mil arquivos levou 1,1x (kernel 6.8) a 1,4x (7.0) do tempo fora do sandbox, contra 2,6x a 3,1x com o supervisor. Sem o auxiliar, o BPF LSM ou o escopo próprio da aba, o supervisor continua, e o `sandmux stats` mostra o motivo. Detalhes em [docs/files.md](https://github.com/alanyoshida/sandmux/blob/main/docs/files.md#pelo-kernel).
- **Reinício do auxiliar sem perder as abas:** se o auxiliar cai ou é reiniciado (uma atualização, por exemplo), a aba fica congelada até o confinamento ser refeito e segue de onde parou. Sem ele de volta em 10 s, a aba para.
- **`sandmux doctor`:** mostra, como informação, se os arquivos das abas podem ir ao kernel nesta máquina.

## Correção

- O confinamento de um Docker privado parado podia ficar aberto no auxiliar root (contando no limite de 16 por usuário); agora sai quando o Docker para.

Esta versão muda o lançador, o daemon e o auxiliar root. O protocolo entre o cliente e o daemon sobe para 43: depois de atualizar, rode `sandmux daemon restart`. Atualize também o auxiliar (`sandmux setup`); um auxiliar antigo deixa as abas com o supervisor.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
