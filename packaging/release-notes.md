sandmux 0.1.10, antes da v1: aba de chat em cartões, com o tema.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Aba de chat em cartões:** a aba de chat com o modelo segue as cores do tema ativo.
  - Cada mensagem fica num cartão de borda arredondada: as suas na segunda cor de destaque, as do modelo na cor de destaque, com o tempo de resposta no canto.
  - O campo da mensagem é um cartão com as dicas no rodapé e o botão `⏎ Enviar` em dois tons, como nos modais.
  - O seletor de `/modelo` também segue o tema.
  - As respostas guardam o tempo que levaram; as conversas de antes abrem sem ele.
- **Chat no modal de nova aba:** o modal de nova aba (Ctrl+b `c`) passa a oferecer `chat` depois dos harnesses. Enter abre o modal de chat, com o modelo e a mensagem.

Detalhes em [docs/chat.md](https://github.com/alanyoshida/sandmux/blob/main/docs/chat.md).

Depois de atualizar, rode `sandmux daemon restart`: a aba de chat é desenhada pelo programa que o daemon lança.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
