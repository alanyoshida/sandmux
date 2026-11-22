## MODIFIED Requirements

### Requirement: Script de instalação
`install.sh` MUST pedir `sudo` logo de início, salvo com `--no-sudo` ou rodado como root. Com privilégio, MUST instalar os binários em `/usr/local/bin`; sem privilégio, em `~/.local/bin`, avisando quando essa pasta não está no `PATH`. A origem MUST ser a release do GitHub, com o SHA-256 conferido antes de instalar, ou uma pasta ou tarball local com `--from`. Com privilégio e as unidades do `sandmux-helper` instaladas em `/etc/systemd/system`, MUST reiniciar o serviço do auxiliar depois de gravar os binários, para o auxiliar em execução passar a ser o novo. Depois MUST rodar `sandmux setup` como o usuário, sem `sudo` quando ele foi recusado. `--uninstall` MUST desabilitar o daemon e remover os binários, o perfil e as unidades do `sandmux-helper`, depois de parar o socket e o serviço.

#### Scenario: Instalação sem privilégio
- **WHEN** o usuário roda `install.sh --no-sudo --from sandmux-0.1.0-x86_64-linux.tar.gz`
- **THEN** os binários ficam em `~/.local/bin`, nada é gravado fora do HOME, e o `sandmux setup` roda sem `sudo`

#### Scenario: Soma errada
- **WHEN** o tarball baixado não confere com o `.sha256` da release
- **THEN** o script para sem instalar nada

#### Scenario: Desinstalar com o auxiliar
- **WHEN** o usuário roda `install.sh --uninstall` com o `sandmux-helper` instalado
- **THEN** o socket e o serviço param, e as unidades e o binário somem

#### Scenario: Atualizar com o auxiliar rodando
- **WHEN** o auxiliar da versão anterior está rodando e o usuário roda `install.sh` com `sudo`
- **THEN** depois de gravar os binários, o serviço do auxiliar é reiniciado, e o auxiliar em execução passa a ser o novo

### Requirement: Auxiliar root pelo setup
Quando o `sandmux-helper` está ao lado do `sandmux`, e as unidades dele não estão instaladas, `sandmux setup` MUST gravar `sandmux-helper.socket` e `sandmux-helper.service` em `/etc/systemd/system`, com o caminho desse binário, e ligar o socket, usando `sudo` depois de perguntar. MUST NOT fazer isso se o binário, ou uma pasta acima dele, puder ser alterado por um usuário que não seja o root; nesse caso, MUST dizer por quê e seguir. Com as unidades já instaladas (pelo setup ou por um pacote) e o auxiliar em execução mais antigo que o sandmux, `sandmux setup` MUST reiniciar o serviço do auxiliar, usando `sudo` depois de perguntar. Com `--no-sudo`, MUST mostrar os comandos.

#### Scenario: Auxiliar instalado pelo setup
- **WHEN** os binários estão em `/usr/local/bin`, do root, e o usuário roda `sandmux setup` e aceita o `sudo`
- **THEN** as unidades do `sandmux-helper` ficam em `/etc/systemd/system` com o caminho `/usr/local/bin/sandmux-helper`, o socket fica ligado, e o `sandmux doctor` mostra o confinamento do Docker por eBPF disponível

#### Scenario: Auxiliar no HOME
- **WHEN** os binários estão em `~/.local/bin` e o usuário roda `sandmux setup`
- **THEN** as unidades do auxiliar não são instaladas, e o setup diz que o binário precisa estar numa pasta do root

#### Scenario: Auxiliar antigo com as unidades em dia
- **WHEN** as unidades do auxiliar estão instaladas e iguais às que o setup gravaria, o auxiliar em execução responde 0.1.30, o sandmux é o 0.1.34, e o usuário roda `sandmux setup` e aceita o `sudo`
- **THEN** o serviço do auxiliar é reiniciado, e o setup diz que reiniciou o auxiliar antigo

#### Scenario: Auxiliar antigo sem sudo
- **WHEN** o auxiliar em execução é mais antigo que o sandmux e o usuário roda `sandmux setup --no-sudo`
- **THEN** o setup mostra o comando `sudo systemctl restart sandmux-helper.service` e não reinicia nada
