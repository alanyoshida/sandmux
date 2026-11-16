sandmux 0.1.15, antes da v1: janelas do desktop, o Chrome com o sandbox dele, harnesses sem perguntas e reinício de aba viva.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Programas com janela** (`--window`, ou `window = true` na configuração): o agente pode abrir janelas no seu desktop Wayland (um navegador, o app que ele construiu) e usar o áudio.
  - **Modo restrito (`true`):** o sandmux registra no compositor um socket da aba (`wp_security_context_v1`) e põe na frente um repassador que esconde os protocolos privilegiados: clipboard em segundo plano, captura de tela, teclado e mouse virtuais, camadas por cima do desktop e controle das outras janelas.
  - **Modo direto (`"direct"`):** o socket comum do compositor, com aviso.
  - **Nunca X11.** O áudio vem junto, e o aviso do lançamento lembra que inclui o microfone.
  - **Visível:** a barra marca a aba com `▣`, e o painel e o `stats` mostram a janela. Detalhes em [docs/window.md](https://github.com/alanyoshida/sandmux/blob/main/docs/window.md).
- **Chrome com o sandbox dele:** o Chrome, o Chromium e os apps Electron sobem dentro do sandmux sem `--no-sandbox`. A correção está no supervisor de arquivos: o mapeamento de uid de namespaces aninhados e o `/proc/self` de pid namespaces aninhados.
- **Harnesses sem perguntas de permissão por padrão:** o opencode, o Claude Code, o hermes e o oh-my-pi rodam sem pedir confirmação, porque o sandbox é a barreira. `harness-asks = true` devolve as perguntas.
- **Reiniciar aba viva:** Ctrl+b `r` numa aba viva pede confirmação e reinicia o harness com a configuração de agora. Na linha de comando, `sandmux restart <id> --force`. Ao mudar a configuração, a TUI e o `config set` avisam quantas abas vivas continuam com o valor antigo.

Depois de atualizar, rode `sandmux daemon restart` (o protocolo passou a 35).

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
