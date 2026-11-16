sandmux 0.1.16, antes da v1: o Antigravity CLI confinado e a tela das abas corrigida.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Antigravity CLI do Google (preset `agy`):** `sandmux run agy` sobe o `agy` confinado, com o binário em somente leitura e a atualização automática desligada; `sandmux update agy` roda `agy update` fora do sandbox.
  - Com `GEMINI_API_KEY` no cofre, o `agy` usa o Gemini pela chave, e o gateway troca a chave falsa pela real. Sem a chave, o login do Google é feito dentro do sandbox.
  - A importação traz as configurações de `~/.gemini` sem o token do login, os segredos de plugins, os projetos, as conversas e os caches.
  - Roda sem perguntas de permissão, como os outros harnesses. Detalhes em [docs/agy.md](https://github.com/alanyoshida/sandmux/blob/main/docs/agy.md).
- **Tela das abas corrigida:** o emulador de terminal das abas passou a tratar a quebra automática de linha desligada (DECAWM, `CSI ?7l`) e a tabulação para trás e para a frente (`CSI Z` e `CSI I`). Programas de tela cheia que usam essas sequências, como o `agy`, não embaralham mais o texto na TUI.

Depois de atualizar, rode `sandmux daemon restart` e abra a TUI de novo.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
