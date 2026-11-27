sandmux 0.1.42, antes da v1: o projeto ganha um site, com a TUI em ação e a documentação de arquitetura e de segurança.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md). The site is in English at [alanyoshida.github.io/sandmux/en/](https://alanyoshida.github.io/sandmux/en/).

## Novidade

- **Site do projeto:** [alanyoshida.github.io/sandmux](https://alanyoshida.github.io/sandmux/), em português e em inglês.
  - A página inicial mostra a TUI gravada de verdade: um `.env` que não abre, um domínio bloqueado e o painel de rede.
  - Toda a documentação de `docs/` fica navegável, com menu lateral e índice de cada página.
- **Arquitetura e segurança:** dois docs novos, [docs/architecture.md](https://github.com/alanyoshida/sandmux/blob/main/docs/architecture.md) e [docs/security.md](https://github.com/alanyoshida/sandmux/blob/main/docs/security.md), escritos para quem avalia o projeto. Explicam as camadas do sandbox, o modelo de ameaça e o que o sandmux não protege.

O programa não muda nesta versão. O protocolo entre o cliente e o daemon também não muda, e o auxiliar root também não.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
