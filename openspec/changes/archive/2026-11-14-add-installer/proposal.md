## Why

Hoje o sandmux só roda a partir de um checkout compilado com `cargo build`, e o perfil do AppArmor, que o Ubuntu exige para o lançador, é colado à mão a partir do `sandmux doctor`. A etapa 10 fecha a v1 com uma instalação de um passo nas distros prometidas (Ubuntu 26.04, Debian 13, Fedora atual e Arch) e com os canais de distribuição decididos na seção 2.1 do roadmap.

## What Changes

- **Binários estáticos:** `sandmux`, `sandmuxd`, `sandmux-run`, `sandmux-chat` e `sandmux-db-mcp` compilados para `x86_64-unknown-linux-musl`, sem depender da glibc da distro. Duas chamadas `ioctl` passam a funcionar com as duas libcs.
- **`sandmux setup`:** o passo de configuração que vale para qualquer canal (script, pacote, `cargo install`):
  - instala o perfil do AppArmor do lançador quando o kernel tem a restrição de user namespace, pedindo `sudo` (ou recusando com o comando, sem `sudo`);
  - habilita e sobe o daemon pela unidade systemd de usuário;
  - termina com o diagnóstico (`sandmux doctor`).
- **Script de instalação (`install.sh`):**
  - pede `sudo` logo de início e, se recusado ou indisponível, instala sem privilégio em `~/.local/bin`;
  - com privilégio, instala em `/usr/local/bin` e o perfil do AppArmor;
  - baixa a release do GitHub e confere o SHA-256, ou instala de uma pasta ou tarball local;
  - roda `sandmux setup` como o usuário;
  - `--uninstall` desfaz.
- **Pacotes:** `scripts/dist.sh` gera o tarball com os binários, o script, o perfil e as licenças; o `.deb` (binários em `/usr/bin` e o perfil carregado no `postinst`); o `.rpm`, quando há `rpmbuild`; o `PKGBUILD` do AUR (`sandmux-bin`) e a fórmula do Homebrew (Linux), com as somas da release.
- **`cargo install`:** `cargo install --git https://github.com/alanyoshida/sandmux sandmux`, seguido de `sandmux setup`.
- **Release:** workflow do GitHub Actions que, numa tag `v*`, gera tudo e cria a release com os arquivos. A primeira release (`v0.1.0`) é publicada neste change, por decisão do usuário. AUR, Homebrew e crates.io ficam preparados para o usuário publicar.
- **Verificação nas distros:** contêineres do Debian 13, do Fedora atual e do Arch no Docker do sistema, sob o vigia de memória. Em cada um: instalação pelo script e pelo pacote nativo, `sandmux doctor` e um sandbox simples.

## Capabilities

### New Capabilities
- `installation`: os binários distribuídos, o `sandmux setup`, o script de instalação com e sem privilégio, os pacotes, a release e a desinstalação.

### Modified Capabilities
(nenhuma: o início automático do daemon e o diagnóstico já existem e não mudam)

## Impact

- **`sandmux`:**
  - `main.rs`: o comando `setup`;
  - `setup.rs` novo;
  - `doctor.rs`: reaproveita o perfil e a detecção da restrição.
- **`sandmux-launcher`:** o `ioctl` do filtro de arquivos com as duas libcs.
- **Arquivos novos:** `packaging/install.sh`, `packaging/deb/`, `packaging/rpm/sandmux.spec`, `packaging/aur/`, `packaging/homebrew/`, `scripts/dist.sh`, `scripts/distro-test.sh` e `.github/workflows/release.yml`.
- **Documentação:** `docs/install.md` novo e o README (seção de instalação).
- **Máquina do usuário:** o alvo musl do Rust foi instalado com a permissão dele. O caminho com `sudo` nesta máquina é verificado à mão pelo usuário.
