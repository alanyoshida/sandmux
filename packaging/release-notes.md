sandmux 0.1.14, antes da v1: a configuração do shell importada.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Configuração do shell no preset `shell`:** o shell confinado pode trazer os seus aliases, funções e o framework do shell, como já acontecia com os harnesses. A primeira execução pergunta; depois, `sandmux import shell`.
  - Entram só os arquivos do shell em uso: no `bash`, `.bashrc`, `.bash_aliases`, `.profile` e afins, com o oh-my-bash e o bash-it; no `zsh`, `.zshrc` e afins, `.p10k.zsh` e o oh-my-zsh; no `fish`, `.config/fish` (sem o `fish_variables`) e o oh-my-fish.
  - Os frameworks vêm sem as pastas `.git`, `cache` e `log`. O `starship.toml` e os arquivos de outros programas ficam de fora.
  - `export GITHUB_TOKEN=...` (e `set -gx` no fish) vira um valor falso; `sandmux vault import` liga o falso ao valor real.
  - O arquivo importado ganha `[sandmux]` na frente do seu prompt.
  - Sem terminal, o `shell` começa sem importar, como antes.

Se o HOME do `shell` já existe, rode `sandmux import shell --overwrite` para trocar o arquivo mínimo pela sua configuração. Detalhes em [docs/sessions.md](https://github.com/alanyoshida/sandmux/blob/main/docs/sessions.md#configuração-do-shell).

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
