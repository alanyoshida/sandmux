sandmux 0.1.6, antes da v1: notificações do desktop.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Notificações do desktop:** o daemon avisa pelo serviço de notificações do sistema:
  - **bloqueio de rede:** a aba, o domínio e o motivo;
  - **chave real** no sandbox: movida ao cofre (reinicie a aba), esperando o cofre ou esperando um domínio;
  - **aba encerrada:** o harness terminou sozinho, com o código. Um `sandmux kill` ou um fechamento seu não avisa;
  - **abas esperando o cofre** depois de um reboot, se ele continua trancado.

  Mais detalhes:
  - Eventos do mesmo tipo na mesma aba, a menos de 60 segundos um do outro, atualizam a mesma notificação, com a contagem.
  - **Só sem foco:** a TUI liga o aviso de foco do terminal, e o daemon não avisa sobre a aba que uma TUI em foco está mostrando. Com a TUI fechada, numa janela de trás ou em outra aba, ele avisa.
  - Vêm ligadas; `notify = false` em `[global]`, ou o item na seção Geral das configurações, desliga.

  Detalhes em [docs/tui.md](https://github.com/alanyoshida/sandmux/blob/main/docs/tui.md#notificações-do-desktop).

Depois de atualizar, rode `sandmux daemon restart` (o protocolo passou a 29).

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
