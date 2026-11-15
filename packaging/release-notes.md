sandmux 0.1.8, antes da v1: daemon no boot.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Daemon no boot:** `sandmux daemon boot on` liga o linger do systemd para o seu usuário (`loginctl enable-linger`, sem sudo), e o daemon passa a subir no boot, sem login.
  - `sandmux daemon boot off` desliga, e `sandmux daemon boot`, `daemon status` e `doctor` mostram o estado.
  - No boot, sem ninguém logado, nenhuma aba volta: elas ficam **aguardando o login**, e o daemon não consulta o keyring, que está trancado.
  - No primeiro login, as abas voltam sozinhas, o daemon tenta abrir o cofre pelo keyring e avisa no desktop as que continuam esperando o cofre.
  - Com o linger já ligado, isso vale também para quem não roda o comando.

  Detalhes em [docs/sessions.md](https://github.com/alanyoshida/sandmux/blob/main/docs/sessions.md#início-no-boot).

Depois de atualizar, rode `sandmux daemon restart` (o protocolo passou a 31).

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
