sandmux 0.1.22, antes da v1: o registro de execuções.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Registro de execuções:** cada programa executado numa aba, pelo agente ou num shell, aparece no registro de arquivos (`sandmux files <id>`, Ctrl+b `f`), com o caminho e os argumentos. Um `curl` lançado por uma ferramenta, um script baixado e rodado, um compilador: agora deixam linha.
  - Com o auxiliar root (`sandmux-helper`) e o BPF LSM ativos, o registro vem do kernel e não dá para falsificar. As execuções do Docker privado da pasta também entram.
  - Sem eles, o supervisor do sandbox registra, lendo o que o processo pediu. Esse registro pode ser enganado, e a linha diz a fonte (`via`).
  - O `sandmux stats` e o painel dizem de onde vêm as execuções da aba.
  - Detalhes em [docs/files.md](https://github.com/alanyoshida/sandmux/blob/main/docs/files.md#execuções).

Esta versão muda o protocolo entre o cliente e o daemon (41) e o auxiliar root. Depois de atualizar, rode `sandmux setup` (que reinicia o auxiliar) e `sandmux daemon restart`, e abra a TUI de novo.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
