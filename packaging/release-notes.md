sandmux 0.1.5, antes da v1: senha do cofre no keyring do sistema.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Keyring:** `sandmux vault keyring on` guarda a senha do cofre no keyring do sistema, pelo Secret Service (gnome-keyring, KWallet, KeePassXC), e o daemon passa a abrir o cofre sozinho:
  - ao subir, antes de atender o primeiro cliente, então a TUI já abre com o cofre aberto;
  - quando um cliente pergunta o estado do cofre trancado, no máximo a cada 5 segundos.

  Mais detalhes:
  - O efeito é o do `sandmux vault unlock`: depois de um reboot, as abas que esperavam o cofre sobem sozinhas.
  - Só a senha vai para o keyring; o cofre continua no arquivo cifrado.
  - Vem desligado, e, desligado, o sandmux nunca consulta o keyring.
  - `sandmux vault keyring off` apaga a senha, e `sandmux vault passwd` a atualiza.
  - Na TUI, a tecla `k` na seção Cofre das configurações liga e desliga.
  - **O custo:** com o keyring ligado, qualquer programa seu fora do sandbox pode ler a senha enquanto o keyring da sessão está aberto. O agente, dentro do sandbox, não alcança o D-Bus.

  Detalhes em [docs/vault.md](https://github.com/alanyoshida/sandmux/blob/main/docs/vault.md#keyring).

Depois de atualizar, rode `sandmux daemon restart` (o protocolo passou a 28).

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
