sandmux 0.1.9, antes da v1: modais do assistente com as cores do tema.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Modais de comando e de chat com o tema:** o modal de comando (Ctrl+b `g`) e o de chat (Ctrl+b `i`) passam a seguir as cores do tema ativo:
  - o fundo e o texto da página;
  - o título num bloco da cor de destaque, como os blocos da barra;
  - os rótulos na cor secundária, e o campo ativo sublinhado no destaque.
- **Botão novo:** `⏎ Enviar` ao perguntar e `⏎ Inserir` com o comando pronto, em dois tons e alinhado à direita. O Tab chega ao botão, e o clique tem o mesmo efeito do Enter.

Detalhes em [docs/chat.md](https://github.com/alanyoshida/sandmux/blob/main/docs/chat.md).

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
