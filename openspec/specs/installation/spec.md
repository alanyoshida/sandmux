# installation Specification

## Purpose
Define como o sandmux é instalado e distribuído: os binários estáticos, o sandmux setup, o script de instalação com e sem privilégio, os pacotes, a release no GitHub e a desinstalação.

## Requirements

### Requirement: Binários estáticos
A distribuição MUST conter os binários `sandmux`, `sandmuxd`, `sandmux-run`, `sandmux-chat`, `sandmux-db-mcp` e `sandmux-helper` ligados estaticamente para `x86_64` Linux, na mesma pasta, sem dependência de bibliotecas da distro.

#### Scenario: Distro sem a glibc da máquina de build
- **WHEN** os binários do tarball são copiados para um contêiner do Arch, do Fedora ou do Debian 13
- **THEN** `sandmux --version` e `sandmux doctor` rodam sem erro de biblioteca

### Requirement: Configuração depois da instalação
`sandmux setup` MUST, quando o kernel tem `kernel.apparmor_restrict_unprivileged_userns` e o perfil do lançador não está carregado, gravar o perfil em `/etc/apparmor.d/sandmux-run` com o caminho do `sandmux-run` instalado e carregá-lo com `apparmor_parser`, usando `sudo` depois de perguntar. Com `--no-sudo`, ou sem `sudo`, MUST mostrar os comandos e seguir. Depois MUST garantir o daemon pela unidade systemd de usuário e terminar com o diagnóstico. Rodado como root, MUST NOT subir o daemon.

#### Scenario: Ubuntu com a restrição
- **WHEN** o usuário roda `sandmux setup` no Ubuntu 26.04 e aceita o `sudo`
- **THEN** o perfil é gravado e carregado, a unidade do daemon é habilitada, e o `sandmux doctor` mostra que o sandbox pode subir

#### Scenario: Sem sudo
- **WHEN** o usuário roda `sandmux setup --no-sudo` numa distro com a restrição
- **THEN** o comando mostra os comandos do perfil para rodar como root e avisa que o sandbox não sobe até lá

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

### Requirement: Pacotes e release
A geração da distribuição MUST produzir o tarball com `.sha256`, um `.deb` com os binários em `/usr/bin` e o perfil do AppArmor carregado na instalação, um `.rpm`, um `PKGBUILD` do pacote `sandmux-bin` e uma fórmula do Homebrew para Linux, com as somas da release. Uma tag `v*` no GitHub MUST gerar esses arquivos e criar a release com eles.

#### Scenario: Pacote do Debian
- **WHEN** o `.deb` é instalado num contêiner do Debian 13
- **THEN** os binários ficam em `/usr/bin`, e `sandmux doctor` roda

#### Scenario: Pacote do Fedora
- **WHEN** o `.rpm` é instalado num contêiner do Fedora atual
- **THEN** os binários ficam em `/usr/bin`, e `sandmux doctor` roda

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

### Requirement: Diagnóstico do kernel sem o Landlock ABI 6
Quando o kernel tem o Landlock abaixo do ABI 6, `sandmux doctor` MUST dizer se o sandbox pode subir pelo auxiliar root (o `sandmux-helper` instalado e o `bpf` entre os LSMs ativos) e, quando não pode, MUST mostrar os dois caminhos: um kernel 6.12 ou mais novo (no Ubuntu 24.04, o kernel HWE, com o comando do `apt`) e o auxiliar com o `bpf` no parâmetro `lsm=` do boot, com os comandos. `sandmux setup` MUST NOT mudar os parâmetros do boot.

#### Scenario: Ubuntu 24.04 com o kernel 6.8
- **WHEN** o usuário roda `sandmux doctor` no Ubuntu 24.04 com o kernel 6.8, sem o `bpf` entre os LSMs ativos
- **THEN** o diagnóstico mostra que o sandbox não sobe, recomenda `sudo apt install linux-generic-hwe-24.04` e mostra como pôr o `bpf` no `lsm=` do GRUB e instalar o auxiliar

#### Scenario: Kernel 6.8 pronto
- **WHEN** o kernel tem o Landlock ABI 4, o `bpf` está entre os LSMs ativos e o auxiliar está instalado
- **THEN** o diagnóstico mostra que o sandbox sobe pelo auxiliar root

#### Scenario: Kernel HWE
- **WHEN** o usuário roda `sandmux doctor` no Ubuntu 24.04 com o kernel HWE 6.14
- **THEN** o diagnóstico mostra o Landlock com o escopo de sockets abstratos e de sinais, sem pedir o auxiliar
