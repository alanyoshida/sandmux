# Instalação

O sandmux é distribuído como binários estáticos para Linux `x86_64`, que não dependem da glibc da distro: `sandmux`, `sandmuxd`, `sandmux-run`, `sandmux-chat` e `sandmux-db-mcp`. Eles ficam na mesma pasta, porque o cliente acha os outros ao lado dele.

Requisitos: kernel 6.12 ou mais novo (Landlock ABI 6), user namespaces sem privilégio e o `/dev/net/tun`. As distros testadas são Ubuntu 26.04, Debian 13, Fedora atual e Arch.

## Script de instalação

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script:

1. Pede `sudo` logo de início.
   - **Com privilégio** (ou rodando como root): instala em `/usr/local/bin`.
   - **Sem privilégio** (`sudo` recusado, ausente ou `--no-sudo`): instala em `~/.local/bin` e avisa quando essa pasta não está no `PATH`.
2. Baixa `sandmux-x86_64-linux.tar.gz` da release e o `.sha256` e confere a soma. Uma soma errada para tudo antes de instalar.
3. Roda `sandmux setup` como o seu usuário (veja abaixo). Quando o `sudo` foi recusado, o setup roda com `--no-sudo`.

| Opção | O que faz |
|---|---|
| `--no-sudo` | Não usa `sudo`: instala em `~/.local/bin` |
| `--prefix DIR` | Pasta dos binários, no lugar do padrão |
| `--from ORIGEM` | Instala de uma pasta extraída ou de um tarball local, sem download |
| `--version vX.Y.Z` | Instala essa versão, no lugar da mais nova |
| `--no-setup` | Não roda `sandmux setup` |
| `--uninstall` | Desabilita o daemon e remove os binários e o perfil do AppArmor |

O tarball também traz o `install.sh`: dentro da pasta extraída, `sh install.sh` instala os binários dela.

`--uninstall` mantém a configuração (`~/.config/sandmux`), os dados (`~/.local/share/sandmux`, com o cofre e os HOMEs sintéticos) e o estado (`~/.local/state/sandmux`, com os registros). Apague-os se quiser.

## `sandmux setup`

É o passo de configuração de todos os canais: script, pacotes, Homebrew e `cargo install`.

1. **Perfil do AppArmor.** Quando o kernel tem a restrição de user namespace pelo AppArmor (`kernel.apparmor_restrict_unprivileged_userns`, ligada por padrão no Ubuntu), o lançador precisa de um perfil próprio, que libera user namespaces só para ele. O setup:
   - grava o perfil em `/etc/apparmor.d/sandmux-run`, com o caminho do `sandmux-run` instalado;
   - carrega o perfil com `apparmor_parser -r`;
   - usa `sudo` depois de perguntar.

   Com `--no-sudo`, sem `sudo` ou sem terminal, ele mostra os dois comandos para rodar como root. O perfil vale para um caminho: se os binários mudarem de lugar, rode `sandmux setup` de novo.
2. **Auxiliar root.** Quando o `sandmux-helper` está ao lado do `sandmux`, numa pasta do root, grava as unidades `sandmux-helper.socket` e `sandmux-helper.service` em `/etc/systemd/system` e liga o socket, com o mesmo `sudo`. É o que liga o confinamento do Docker privado por eBPF ([helper.md](helper.md)). Num binário que o usuário pode alterar, como em `~/.local/bin`, ele recusa e diz por quê. Com `--no-sudo`, mostra os comandos.
3. **Daemon.** Instala e habilita a unidade systemd de usuário `sandmuxd.service`, que sobe o daemon no login ([sessions.md](sessions.md)). Para ele subir já no boot, sem login, rode `sandmux daemon boot on` ([sessions.md](sessions.md#início-no-boot)). `--no-daemon` pula esse passo, e como root ele nunca roda, porque o daemon é por usuário.
4. **Diagnóstico.** Termina com o `sandmux doctor`.

## Pacotes

| Canal | Arquivo | Instala em | Depois |
|---|---|---|---|
| Debian e Ubuntu | `sandmux_<versão>_amd64.deb` | `/usr/bin`, perfil em `/etc/apparmor.d/sandmux-run`, carregado na instalação, e as unidades do `sandmux-helper` em `/usr/lib/systemd/system`, com o socket ligado | `sandmux setup` como o usuário |
| Fedora | `sandmux-<versão>-1.x86_64.rpm` | `/usr/bin`, perfil em `/etc/apparmor.d/sandmux-run`, unidades do `sandmux-helper` com o socket ligado | `sandmux setup` como o usuário |
| Arch (AUR) | `sandmux-bin` (`PKGBUILD` da release) | `/usr/bin`, unidades do `sandmux-helper` com o socket ligado | `sandmux setup` como o usuário |
| Homebrew (Linux) | fórmula `sandmux.rb` da release | a pasta do Homebrew, sem o `sandmux-helper` | `sandmux setup` (o Homebrew não roda `sudo`) |
| Tarball | `sandmux-<versão>-x86_64-linux.tar.gz` | onde o `install.sh` mandar | o `install.sh` já roda o setup |

```sh
sudo apt install ./sandmux_0.1.21_amd64.deb
sudo dnf install ./sandmux-0.1.21-1.x86_64.rpm
```

## Pelo código

Precisa de Rust 1.88 ou mais novo.

```sh
cargo install --git https://github.com/alanyoshida/sandmux sandmux
sandmux setup
```

O `cargo install` instala os binários juntos em `~/.cargo/bin`.

Para compilar um checkout e gerar a mesma distribuição da release (binários estáticos, tarballs, `.deb`, `.rpm` quando há `rpmbuild`, `PKGBUILD` e fórmula em `dist/`):

```sh
rustup target add x86_64-unknown-linux-musl
./scripts/dist.sh
```

O C do `ring` compila com o gcc do sistema (`CC_x86_64_unknown_linux_musl=gcc`, que o script já define), sem o `musl-gcc`.

## Release

Uma tag `v*` no GitHub roda `.github/workflows/release.yml`. O workflow:

1. gera a distribuição com `scripts/dist.sh`, num Ubuntu com `musl-tools` e `rpm`;
2. confere que a tag é a versão do `Cargo.toml`;
3. cria a release como a mais nova (`--latest`, para que `/releases/latest/` sirva o `install.sh`) com o tarball, as somas, o `.deb`, o `.rpm`, o `install.sh`, o `PKGBUILD`, o `SRCINFO` (o `.SRCINFO` do AUR, sem o ponto, que o GitHub tiraria) e a fórmula.

Publicar no AUR, num tap do Homebrew e no crates.io fica a cargo do mantenedor, com as contas dele. Os arquivos saem prontos em `dist/aur/` e `dist/homebrew/`.

## Teste nas distros

`scripts/distro-test.sh` testa `dist/` em contêineres do Debian 13, do Fedora atual e do Arch no Docker do sistema. As opções `seccomp=unconfined`, `apparmor=unconfined` e `systempaths=unconfined` são necessárias porque o Docker bloqueia o user namespace e a montagem de `/proc` que o sandbox usa. Em cada contêiner:

- **como root:** instala o pacote nativo (no Arch, o tarball pelo `install.sh`, e confere o `.SRCINFO` com o `makepkg`) e roda `sandmux --version`;
- **como um usuário comum:** `install.sh --no-sudo`, `sandmux doctor` e um sandbox sem rede;
- **no fim:** remove o pacote.

No Fedora, o script gera também o `.rpm` em `dist/`.

O kernel, o Landlock e o AppArmor são os da máquina que roda os contêineres: o teste prova os binários, os pacotes e o espaço de usuário de cada distro.
