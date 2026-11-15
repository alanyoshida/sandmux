sandmux 0.1.12, antes da v1: painéis na aba, como no tmux.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Painéis na aba:** uma aba pode ter vários painéis, cada um com a sua sessão.
  - **Dividir:** Ctrl+b `%` divide o painel ativo lado a lado, e Ctrl+b `"` em cima e embaixo. O modal de nova aba abre com a pasta do painel, e a sessão criada (um harness, o shell ou o chat) ocupa a parte nova.
  - **Foco:** Ctrl+b com as setas vai ao painel vizinho, e um clique também põe o foco nele.
  - **Ampliar:** Ctrl+b `z` mostra só o painel ativo; de novo, volta ao layout.
  - **Tamanho:** Ctrl+b `<` e `>` diminuem e aumentam o painel ativo.
  - **Fechar:** fechar o painel dá o espaço e o foco ao vizinho, na mesma aba.
  - **Barra:** uma entrada por aba, com `+N` para os outros painéis.
  - **Layout:** fica no daemon. Volta ao reabrir a TUI e depois de um reboot.

  Detalhes em [docs/tui.md](https://github.com/alanyoshida/sandmux/blob/main/docs/tui.md#painéis).
- **Correção:** um `sandmux daemon stop` ou `restart` às vezes não conseguia subir o daemon de novo ("o daemon já está rodando"), porque o anterior ainda segurava a trava por um instante. Agora a troca espera a trava ficar livre.

Depois de atualizar, rode `sandmux daemon restart` (o protocolo passou a 32).

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
