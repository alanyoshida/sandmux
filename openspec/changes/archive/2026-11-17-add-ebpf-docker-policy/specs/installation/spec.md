## MODIFIED Requirements

### Requirement: Binários estáticos
A distribuição MUST conter os binários `sandmux`, `sandmuxd`, `sandmux-run`, `sandmux-chat`, `sandmux-db-mcp` e `sandmux-helper` ligados estaticamente para `x86_64` Linux, na mesma pasta, sem dependência de bibliotecas da distro.

#### Scenario: Distro sem a glibc da máquina de build
- **WHEN** os binários do tarball são copiados para um contêiner do Arch, do Fedora ou do Debian 13
- **THEN** `sandmux --version` e `sandmux doctor` rodam sem erro de biblioteca

### Requirement: Script de instalação
`install.sh` MUST pedir `sudo` logo de início, salvo com `--no-sudo` ou rodado como root. Com privilégio, MUST instalar os binários em `/usr/local/bin`; sem privilégio, em `~/.local/bin`, avisando quando essa pasta não está no `PATH`. A origem MUST ser a release do GitHub, com o SHA-256 conferido antes de instalar, ou uma pasta ou tarball local com `--from`. Depois MUST rodar `sandmux setup` como o usuário, sem `sudo` quando ele foi recusado. `--uninstall` MUST desabilitar o daemon e remover os binários, o perfil e as unidades do `sandmux-helper`, depois de parar o socket e o serviço.

#### Scenario: Instalação sem privilégio
- **WHEN** o usuário roda `install.sh --no-sudo --from sandmux-0.1.0-x86_64-linux.tar.gz`
- **THEN** os binários ficam em `~/.local/bin`, nada é gravado fora do HOME, e o `sandmux setup` roda sem `sudo`

#### Scenario: Soma errada
- **WHEN** o tarball baixado não confere com o `.sha256` da release
- **THEN** o script para sem instalar nada

#### Scenario: Desinstalar com o auxiliar
- **WHEN** o usuário roda `install.sh --uninstall` com o `sandmux-helper` instalado
- **THEN** o socket e o serviço param, e as unidades e o binário somem

## ADDED Requirements

### Requirement: Auxiliar root pelo setup
Quando o `sandmux-helper` está ao lado do `sandmux`, e as unidades dele não estão instaladas, `sandmux setup` MUST gravar `sandmux-helper.socket` e `sandmux-helper.service` em `/etc/systemd/system`, com o caminho desse binário, e ligar o socket, usando `sudo` depois de perguntar. MUST NOT fazer isso se o binário, ou uma pasta acima dele, puder ser alterado por um usuário que não seja o root; nesse caso, MUST dizer por quê e seguir. Com `--no-sudo`, MUST mostrar os comandos.

#### Scenario: Auxiliar instalado pelo setup
- **WHEN** os binários estão em `/usr/local/bin`, do root, e o usuário roda `sandmux setup` e aceita o `sudo`
- **THEN** as unidades do `sandmux-helper` ficam em `/etc/systemd/system` com o caminho `/usr/local/bin/sandmux-helper`, o socket fica ligado, e o `sandmux doctor` mostra o confinamento do Docker por eBPF disponível

#### Scenario: Auxiliar no HOME
- **WHEN** os binários estão em `~/.local/bin` e o usuário roda `sandmux setup`
- **THEN** as unidades do auxiliar não são instaladas, e o setup diz que o binário precisa estar numa pasta do root

### Requirement: Unidades do auxiliar nos pacotes
O `.deb`, o `.rpm` e o `PKGBUILD` MUST trazer o `sandmux-helper` e as unidades dele em `/usr/lib/systemd/system`, com o caminho `/usr/bin/sandmux-helper`, e ligar o socket na instalação. A fórmula do Homebrew MUST NOT trazer o auxiliar nem as unidades, porque o Homebrew não roda como root.

#### Scenario: Pacote do Debian com o auxiliar
- **WHEN** o `.deb` é instalado num Debian 13 com systemd
- **THEN** as unidades do `sandmux-helper` ficam em `/usr/lib/systemd/system`, e o socket fica ligado
