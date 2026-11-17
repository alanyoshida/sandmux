sandmux 0.1.18, antes da v1: o portal do desktop e a troca de uma chave vencida.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Portal do desktop:** toda aba ganha, por padrão, um D-Bus próprio do sandmux, que não é o seu.
  - **Tema:** os programas de dentro leem o tema claro ou escuro do seu desktop.
  - **Notificações:** o `notify-send` e o portal chegam ao seu desktop com o nome da aba no título, no máximo 5 a cada 10 segundos por aba.
  - **Abrir link:** o `xdg-open` abre links no seu navegador, mas só para domínios que a política de rede da aba permite; cada pedido entra em `sandmux net`.
  - Qualquer outro serviço do D-Bus recebe erro, e nada do sandbox chega ao seu D-Bus.
  - Para desligar, use `--no-portal` ou `portal = false`. Detalhes em [docs/window.md](https://github.com/alanyoshida/sandmux/blob/main/docs/window.md#portal-do-desktop).
- **Trocar uma chave que venceu:** `sandmux vault set <nome>`, ou a tecla `v` na seção Cofre da TUI, troca só o valor e mantém o falso e os domínios. Os arquivos do harness continuam valendo, e o valor novo vale na próxima conexão, sem reiniciar a aba. Detalhes em [docs/vault.md](https://github.com/alanyoshida/sandmux/blob/main/docs/vault.md#trocar-uma-chave-que-venceu).
- **Teste mais estável:** um teste do Docker que falhava às vezes foi corrigido; a causa era o nome repetido de um arquivo de saída do próprio teste.

Esta versão muda o protocolo entre o cliente e o daemon (38). Depois de atualizar, rode `sandmux daemon restart` e abra a TUI de novo.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
