sandmux 0.1.7, antes da v1: grupos de abas.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Grupos de abas,** como as sessões do tmux. Toda aba pertence a um grupo, e cada TUI mostra um grupo por vez:
  - `sandmux run <harness> --group estudos` cria a aba no grupo; sem `--group`, ela vai para o `principal`, onde também ficam as abas que já existiam;
  - `sandmux ls` ganha a coluna GRUPO, e `--group` filtra;
  - `sandmux group list`, `group move <id> <grupo>` e `group rename <grupo> <novo>`;
  - `sandmux tui --group estudos` abre a TUI no grupo.

  Na TUI:
  - Ctrl+b `(` e `)` trocam de grupo, e as abas do grupo que sai continuam vivas;
  - Ctrl+b `G` abre a lista de grupos: Enter troca, `n` cria, `r` renomeia, `m` leva a aba ativa;
  - uma aba nova vai para o grupo atual;
  - duas janelas podem mostrar grupos diferentes.

  Um grupo existe enquanto tem abas, e volta com elas depois de um reboot. Detalhes em [docs/sessions.md](https://github.com/alanyoshida/sandmux/blob/main/docs/sessions.md#grupos-de-abas).

Depois de atualizar, rode `sandmux daemon restart` (o protocolo passou a 30).

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
