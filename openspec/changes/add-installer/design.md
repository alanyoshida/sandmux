## Context

O que já existe:

- **Binários:** quatro de produção mais o `sandmux-chat` e o `sandmux-db-mcp`. O cliente acha os outros ao lado dele (`launch::sibling`).
- **`sandmux doctor`:** detecta a restrição do AppArmor e mostra o perfil do lançador já preenchido, com o comando de instalação.
- **Daemon:** o cliente instala e habilita a unidade systemd de usuário na primeira vez que precisa dele.
- **Proteção:** a checagem de nível roda a cada lançamento (`support::check_via`).

O workspace tem `publish = false` e versão `0.1.0`. O repositório é `github.com/alanyoshida/sandmux`.

Decisões do usuário em 2026-11-14:

- preparar os canais e publicar a release no GitHub;
- testar as distros em contêineres;
- binário estático com musl;
- o caminho com `sudo` nesta máquina fica para ele verificar à mão.

## Goals / Non-Goals

**Goals:**
- Instalação de um passo, com e sem privilégio, nas quatro distros.
- Um só passo de configuração (`sandmux setup`) para todos os canais.
- A release `v0.1.0` publicada no GitHub, com o tarball, o `.deb`, o `.rpm` e as somas.

**Non-Goals:**
- **`aarch64`:** fica para depois, porque precisa de um toolchain cruzado de musl.
- **Publicar no AUR, no Homebrew e no crates.io:** os arquivos ficam prontos, e o usuário publica com as contas dele.
- **`sandmux-helper` e eBPF:** depois da v1.
- **Atualização automática.** Instalar a versão nova por cima é a atualização.

## Decisions

### Binários estáticos com musl

`cargo build --release --target x86_64-unknown-linux-musl`, com `CC_x86_64_unknown_linux_musl=gcc` para o `ring`: o C dele compila com o gcc do sistema, sem o `musl-gcc`.

O `ioctl` da libc musl recebe `int` como pedido, e o da glibc recebe `unsigned long`. As chamadas com constantes próprias (`IOCTL_*` do filtro de arquivos e a do `sandmux-probe`) passam a converter com `as _`. A conversão mantém os bits do pedido.

Os binários saem sem símbolos (`strip`). O `sandmux-probe` é só dos testes e não entra.

### `sandmux setup`

Um comando no cliente, para não repetir a lógica no script, nos pacotes e no `cargo install`. Ele faz três passos.

**1. Perfil do AppArmor.** O passo só roda quando existe `/proc/sys/kernel/apparmor_restrict_unprivileged_userns` (kernel com a restrição) e `apparmor_parser`. Ele compara o texto do perfil, preenchido com o caminho real do `sandmux-run` instalado, com `/etc/apparmor.d/sandmux-run`:

- **igual:** recarrega só se não estiver carregado (lista em `/sys/kernel/security/apparmor/profiles`);
- **diferente:** pergunta, quando há terminal, e roda `sudo tee` e `sudo apparmor_parser -r`;
- **com `--no-sudo`, sem `sudo` ou sem terminal:** mostra os dois comandos, como o `doctor`.

O perfil é instalado mesmo com a restrição em 0, porque o Ubuntu a liga por padrão e um administrador pode religá-la.

**2. Daemon.** O passo usa `client::connect_or_start()`, que já instala e habilita a unidade, com o aviso de sempre. Como root, ele é pulado: o daemon é por usuário.

**3. Diagnóstico.** O passo roda `doctor::run()`.

### `install.sh`

Script POSIX `sh`, servido como arquivo da release, e também dentro do tarball.

**Privilégio:**
1. root roda direto;
2. com `--no-sudo`, segue sem privilégio;
3. com `sudo`, roda `sudo -v` logo de início; se falhar, segue sem privilégio, com aviso.

**Destino:** `/usr/local/bin` com privilégio; `~/.local/bin` sem ele (com `--prefix` para trocar).

**Origem:**
- `--from <pasta|tarball>`;
- ao lado do script, quando ele está num tarball extraído (`bin/sandmux` existe);
- sem nada disso, a release `latest` (ou `--version vX`) do GitHub, por `curl` ou `wget`. Baixa `sandmux-x86_64-linux.tar.gz` e o `.sha256` e confere com `sha256sum` antes de extrair.

**Arquitetura:** só `x86_64`; outra é recusada com a mensagem.

**Depois de copiar:** roda `<destino>/sandmux setup` como o usuário que chamou o script. Com o `sudo` recusado, passa `--no-sudo`. Como root sem usuário (contêiner), passa `--no-daemon`.

**`--uninstall`:**
1. roda `sandmux daemon disable`, quando o binário existe;
2. remove os binários de `/usr/local/bin` ou `~/.local/bin`;
3. remove o perfil, com privilégio.

A configuração e os dados do usuário ficam, e o script diz onde estão.

### Pacotes

`scripts/dist.sh <versão>` monta tudo em `dist/`:

- **Tarball:** `sandmux-<versão>-x86_64-linux.tar.gz`, mais uma cópia com o nome fixo `sandmux-x86_64-linux.tar.gz` para o `latest`, e um `.sha256` para cada. Dentro: `bin/`, `install.sh`, `apparmor/sandmux-run`, as licenças e o README.
- **`.deb`:** montado com `dpkg-deb`, sem ferramenta nova.
  - Binários em `/usr/bin` e o perfil em `/etc/apparmor.d/sandmux-run` (já com `/usr/bin/sandmux-run`).
  - O `postinst` carrega o perfil quando há `apparmor_parser`; o `postrm` o descarrega e o remove no `purge`.
  - `Depends:` vazio, porque os binários são estáticos.
- **`.rpm`:** por `rpmbuild` com `packaging/rpm/sandmux.spec`, quando ele existe. Na máquina sem `rpmbuild`, o `.rpm` é gerado no contêiner do Fedora pelo teste das distros, e no CI pelo pacote `rpm` do Ubuntu.
- **AUR:** `packaging/aur/PKGBUILD` do `sandmux-bin`, que baixa o tarball versionado, e o `.SRCINFO`. A versão e a soma são preenchidas pelo `dist.sh` em `dist/aur/`.
- **Homebrew:** `packaging/homebrew/sandmux.rb` com `depends_on :linux`, preenchida em `dist/homebrew/`. O Homebrew não roda `sudo`, então a fórmula manda rodar `sandmux setup` (nos `caveats`).
- **`cargo install`:** pelo git. O `README` e o `docs/install.md` mandam rodar `sandmux setup` depois.

### Release no GitHub

O `.github/workflows/release.yml` roda numa tag `v*`, em `ubuntu-latest`:

1. instala `musl-tools`, o alvo musl e o pacote `rpm`;
2. roda `scripts/dist.sh`;
3. publica com `gh release create` e os arquivos de `dist/`.

A primeira release pode ser criada localmente com o mesmo `dist.sh` e o `gh`. Por isso a publicação não depende do workflow rodar.

### Teste nas distros

`scripts/distro-test.sh` roda, para `debian:13`, `fedora:latest` e `archlinux:latest`, um contêiner com:

- **Opções de segurança:** `--security-opt seccomp=unconfined --security-opt apparmor=unconfined --security-opt systempaths=unconfined`, porque o Docker bloqueia por padrão user namespaces e a montagem de `/proc` que o sandbox usa.
- **Teto:** `--memory 2g`.
- **Usuário comum** criado na imagem.

Cada contêiner faz:

1. **como root:** instala o pacote nativo (`.deb`, `.rpm` ou o tarball no Arch, onde roda também `makepkg --printsrcinfo`) e roda `sandmux doctor`;
2. **como o usuário:** `install.sh --no-sudo --from` o tarball, `sandmux doctor` e `sandmux run shell` sem terminal com `echo ok`.

O kernel e o AppArmor são os da máquina.

## Risks / Trade-offs

- **[O teste em contêiner não troca o kernel]** Landlock, seccomp e user namespace são os do kernel desta máquina. O teste prova os binários, os pacotes e o espaço de usuário de cada distro, não o kernel delas. Fica registrado nas verificações do roadmap.
- **[`sudo -v` logo de início pede a senha mesmo quando o perfil não é preciso]** É a decisão 2.1 ("pede `sudo` logo de início"). `--no-sudo` evita.
- **[Release publicada antes da v1]** A `v0.1.0` é marcada como pré-release no GitHub.
- **[Script baixado e executado (`curl | sh`)]** A soma confere o tarball. O script vem do mesmo lugar e mostra o que vai fazer antes do `sudo`.
