# sandbox-filesystem Specification

## Purpose

Define o que um processo confinado pelo sandmux enxerga do disco: a pasta de trabalho, o HOME sintético, o sistema em somente leitura, as toolchains e os caches do usuário, e o que fica escondido ou protegido contra escrita.

## Requirements

### Requirement: Pasta de trabalho é a pasta atual
A pasta de trabalho MUST ser, por padrão, a pasta de onde `sandmux run` foi executado. Ela MUST aparecer dentro do sandbox no mesmo caminho absoluto, com leitura e escrita, e ser a pasta real do host, sem cópia nem camada intermediária. A opção `--workdir <pasta>` MUST indicar outra pasta de trabalho.

#### Scenario: Arquivo criado pelo agente
- **WHEN** um processo confinado cria um arquivo na pasta de trabalho
- **THEN** o arquivo existe no host, no mesmo caminho, assim que é criado

#### Scenario: Arquivo que já estava na pasta
- **WHEN** a pasta de trabalho contém um PDF antes do lançamento
- **THEN** o processo confinado lê o PDF no mesmo caminho

#### Scenario: Arquivo alterado no host
- **WHEN** o usuário edita no host um arquivo da pasta de trabalho com o sandbox rodando
- **THEN** o processo confinado vê o conteúdo novo

#### Scenario: Outra pasta de trabalho
- **WHEN** o usuário roda `sandmux run opencode --workdir ~/projetos/app` a partir de outra pasta
- **THEN** a pasta de trabalho é `~/projetos/app` e o harness inicia dentro dela

### Requirement: Pasta de trabalho perigosa é recusada
`sandmux run` MUST recusar o lançamento quando a pasta de trabalho é o HOME do usuário, a raiz do sistema, uma pasta que contém o HOME, uma pasta dentro do diretório de dados do sandmux ou uma pasta que o contém. A mesma recusa MUST valer para caminhos passados em `--ro` e `--rw`. Não existe opção para forçar.

#### Scenario: Lançamento no HOME
- **WHEN** o usuário roda `sandmux run opencode` com o HOME como pasta atual
- **THEN** o `run` termina com erro, explica o motivo, sugere usar uma subpasta e não lança o sandbox

#### Scenario: Pasta que contém o HOME
- **WHEN** a pasta de trabalho é `/` ou `/home`
- **THEN** o `run` termina com erro e não lança o sandbox

#### Scenario: Montagem extra perigosa
- **WHEN** o usuário passa `--ro ~` ou `--rw /`
- **THEN** o `run` termina com erro e não lança o sandbox

#### Scenario: Subpasta do HOME
- **WHEN** a pasta de trabalho é uma subpasta do HOME
- **THEN** o lançamento prossegue

### Requirement: HOME real fora do alcance
Dentro do sandbox, o caminho do HOME do usuário MUST apontar para um HOME sintético. Nada do HOME real MUST ser visível, exceto a pasta de trabalho, as toolchains e os caches conhecidos, a pasta de entrada e o que o usuário acrescentou com `--ro` ou `--rw`.

#### Scenario: Chaves SSH
- **WHEN** um processo confinado tenta ler `~/.ssh`
- **THEN** o caminho não existe

#### Scenario: Outro projeto
- **WHEN** um processo confinado tenta ler uma pasta do HOME real que não é a pasta de trabalho
- **THEN** o caminho não existe

#### Scenario: Estado de outro harness
- **WHEN** um processo confinado lançado pelo preset do opencode tenta ler `~/.hermes/.env`
- **THEN** o caminho não existe

### Requirement: HOME sintético por harness
Cada harness MUST ter um HOME sintético próprio, guardado no diretório de dados do sandmux, com leitura e escrita, que persiste entre execuções e é compartilhado por execuções simultâneas do mesmo harness, em qualquer pasta de trabalho.

#### Scenario: Persistência
- **WHEN** um processo confinado grava um arquivo no HOME e o sandbox é encerrado
- **THEN** uma nova execução do mesmo harness encontra o arquivo

#### Scenario: Harnesses diferentes
- **WHEN** o opencode e o hermes rodam confinados
- **THEN** cada um vê um HOME diferente

### Requirement: Sistema em somente leitura
As pastas de sistema do host (`/usr`, `/bin`, `/sbin`, `/lib*`, `/opt`, `/etc`, `/sys`) MUST aparecer em somente leitura. `/proc`, `/tmp`, `/run` e `/var/tmp` MUST ser novos e privados do sandbox. Nenhuma outra pasta do host MUST existir dentro do sandbox por padrão.

#### Scenario: Escrita no sistema
- **WHEN** um processo confinado tenta criar ou alterar um arquivo em `/usr` ou `/etc`
- **THEN** a operação falha

#### Scenario: Arquivos temporários do host
- **WHEN** um processo confinado lista `/tmp` e `/run`
- **THEN** não aparece nenhum arquivo ou socket do host

#### Scenario: Pasta fora da visão
- **WHEN** um processo confinado tenta ler `/mnt`, `/media`, `/srv` ou `/root`
- **THEN** o caminho não existe

### Requirement: Contas do sistema substituídas
`/etc/passwd` e `/etc/group` MUST ser substituídos por versões geradas que contêm apenas o usuário do sandbox, `root` e `nobody`, e os grupos correspondentes. O usuário do sandbox MUST manter o nome, o UID, o GID e o caminho do HOME do usuário real.

#### Scenario: Outros usuários da máquina
- **WHEN** um processo confinado lê `/etc/passwd`
- **THEN** não aparecem as outras contas do host

#### Scenario: Nome do usuário
- **WHEN** um programa confinado resolve o nome do usuário atual
- **THEN** obtém o mesmo nome que o usuário tem no host

### Requirement: Arquivos de senha e de privilégio de /etc cobertos
Os seguintes caminhos de `/etc` MUST aparecer vazios dentro do sandbox: `shadow`, `shadow-`, `gshadow`, `gshadow-`, `security/opasswd`, `sudoers`, `sudoers.d`, `environment`.

#### Scenario: Regras de sudo
- **WHEN** um processo confinado lista `/etc/sudoers.d`
- **THEN** a pasta aparece vazia

#### Scenario: Variáveis globais do sistema
- **WHEN** um processo confinado lê `/etc/environment`
- **THEN** o arquivo aparece vazio

### Requirement: Chaves e credenciais de serviço de /etc cobertas
Os seguintes caminhos de `/etc` MUST aparecer vazios dentro do sandbox: as chaves privadas de host em `ssh` (`ssh_host_*_key`), `ssl/private`, `pki/tls/private`, `krb5.keytab`, `ipsec.secrets`, `ipsec.d`, `letsencrypt`, `apt/auth.conf`, `apt/auth.conf.d`, `docker`, `kubernetes`, `rancher`, `libvirt`, `iscsi`, `samba`, `cups`, `davfs2`, `msmtprc`, `postfix/sasl_passwd`, `crypttab`.

#### Scenario: Chave privada de TLS
- **WHEN** um processo confinado tenta listar `/etc/ssl/private`
- **THEN** a pasta aparece vazia

#### Scenario: Certificados públicos
- **WHEN** um programa confinado valida um certificado TLS usando `/etc/ssl/certs`
- **THEN** a validação funciona

### Requirement: Credenciais de rede de /etc cobertas
Os seguintes caminhos de `/etc` MUST aparecer vazios dentro do sandbox: `NetworkManager/system-connections`, `netplan`, `wpa_supplicant`, `wireguard`, `openvpn`, `ppp`.

#### Scenario: Senhas de Wi-Fi
- **WHEN** um processo confinado lista `/etc/NetworkManager/system-connections`
- **THEN** a pasta aparece vazia

### Requirement: Resolução de nomes funciona
A resolução de nomes MUST funcionar dentro do sandbox com a configuração do host, mesmo quando `/etc/resolv.conf` é um link para um arquivo em `/run`.

#### Scenario: Link para /run
- **WHEN** `/etc/resolv.conf` do host aponta para um arquivo em `/run/systemd/resolve`
- **THEN** um programa confinado resolve um nome de domínio público

### Requirement: Dispositivos mínimos, com GPU
O `/dev` do sandbox MUST conter apenas `null`, `zero`, `full`, `random`, `urandom`, `tty` e os pseudoterminais, mais os dispositivos de GPU presentes na máquina: os nós de renderização de `/dev/dri` e os nós do driver da NVIDIA, exceto o de controle de modo de vídeo. `/dev/kvm` MUST entrar somente quando a política efetiva liga `kvm`, pela opção `--kvm` ou pela configuração. A GPU MUST sair quando a política efetiva desliga `gpu`, pela opção `--no-gpu` ou pela configuração.

#### Scenario: Discos do host
- **WHEN** um processo confinado lista `/dev`
- **THEN** não aparecem discos, câmeras, dispositivos de entrada nem `/dev/kvm`

#### Scenario: GPU presente
- **WHEN** a máquina tem GPU e o sandbox é lançado sem opções
- **THEN** um programa confinado enxerga a GPU

#### Scenario: Máquina sem GPU
- **WHEN** a máquina não tem GPU
- **THEN** o lançamento prossegue sem aviso

#### Scenario: GPU desligada
- **WHEN** o usuário passa `--no-gpu`
- **THEN** nenhum dispositivo de GPU existe dentro do sandbox

#### Scenario: KVM por opção
- **WHEN** o usuário passa `--kvm` numa máquina com `/dev/kvm`
- **THEN** `/dev/kvm` existe dentro do sandbox

#### Scenario: KVM pela configuração
- **WHEN** o perfil do projeto tem `kvm = true` e o sandbox é lançado na pasta do projeto, sem opções, numa máquina com `/dev/kvm`
- **THEN** `/dev/kvm` existe dentro do sandbox

#### Scenario: GPU desligada pela configuração
- **WHEN** `[global]` tem `gpu = false` e o sandbox é lançado sem opções numa máquina com GPU
- **THEN** nenhum dispositivo de GPU existe dentro do sandbox

### Requirement: Toolchains do HOME por lista conhecida
Do HOME real, só MUST entrar no sandbox as toolchains de uma lista conhecida, cada uma no caminho original. Uma toolchain da lista que não está instalada MUST ser ignorada sem erro. O `PATH` dentro do sandbox MUST ser reconstruído com as pastas do sistema e as pastas de binários das toolchains presentes.

#### Scenario: Toolchain instalada
- **WHEN** o usuário tem o cargo instalado em `~/.cargo`
- **THEN** `cargo` é encontrado no `PATH` dentro do sandbox e funciona

#### Scenario: Toolchain ausente
- **WHEN** o usuário não tem o pyenv instalado
- **THEN** o lançamento prossegue e `~/.pyenv` não existe dentro do sandbox

#### Scenario: Ferramenta fora da lista
- **WHEN** o usuário tem em `~/.local/bin` um binário que não é de nenhuma toolchain da lista
- **THEN** o binário não existe dentro do sandbox até ser acrescentado com `--ro`

### Requirement: Entradas de Node e Bun na lista de toolchains
A lista de toolchains MUST incluir: nvm, com a pasta `~/.nvm` e a versão padrão do Node no `PATH`; e bun, com a pasta `~/.bun` e `~/.bun/bin` no `PATH`. O arquivo `~/.npmrc` e o `~/.bunfig.toml` do HOME real MUST NOT entrar.

#### Scenario: Node do nvm
- **WHEN** o usuário tem o Node instalado pelo nvm
- **THEN** `node` e `npm` da versão padrão do nvm funcionam dentro do sandbox

#### Scenario: Token do npm
- **WHEN** o `~/.npmrc` real contém um token de registry
- **THEN** o arquivo não existe dentro do sandbox

### Requirement: Entradas de Rust na lista de toolchains
A lista de toolchains MUST incluir: cargo, com a pasta `~/.cargo` e `~/.cargo/bin` no `PATH`; e rustup, com a pasta `~/.rustup`. Os arquivos `~/.cargo/credentials.toml` e `~/.cargo/credentials` MUST aparecer vazios.

#### Scenario: Compilação
- **WHEN** um processo confinado roda `cargo build` num projeto da pasta de trabalho
- **THEN** a compilação usa a toolchain do rustup do usuário

#### Scenario: Token do crates.io
- **WHEN** um processo confinado lê `~/.cargo/credentials.toml`
- **THEN** o arquivo aparece vazio

### Requirement: Entradas de Python na lista de toolchains
A lista de toolchains MUST incluir: uv, com os binários `uv` e `uvx` de `~/.local/bin` e a pasta `~/.local/share/uv`; e pyenv, com a pasta `~/.pyenv` e as pastas de shims e de binários no `PATH`. A pasta `~/.local/bin` MUST NOT entrar inteira. A pasta `~/.local/share/uv/credentials` MUST aparecer vazia.

#### Scenario: Python gerenciado pelo uv
- **WHEN** um ambiente virtual da pasta de trabalho aponta para um Python instalado pelo uv
- **THEN** o ambiente funciona dentro do sandbox

#### Scenario: Outros binários de ~/.local/bin
- **WHEN** `~/.local/bin` contém `kubectl` e `helm`
- **THEN** nenhum dos dois existe dentro do sandbox

### Requirement: Entradas de Go e Homebrew na lista de toolchains
A lista de toolchains MUST incluir: go, com a pasta `~/go` e `~/go/bin` no `PATH`; e linuxbrew, com a pasta `/home/linuxbrew/.linuxbrew` ou `~/.linuxbrew` e as pastas `bin` e `sbin` no `PATH`.

#### Scenario: Ferramenta instalada com go install
- **WHEN** o usuário tem um binário em `~/go/bin`
- **THEN** o binário é encontrado no `PATH` dentro do sandbox

### Requirement: Camada privada de escrita nas toolchains
As pastas de toolchain MUST ser lidas do host e escritas numa camada privada do harness, guardada no diretório de dados do sandmux. O que o processo confinado instala MUST funcionar dentro do sandbox, MUST persistir nas execuções seguintes do mesmo harness e MUST NOT alterar a pasta do host.

#### Scenario: Instalação global
- **WHEN** um processo confinado roda `cargo install`, `npm install -g`, `rustup component add` ou `go install`
- **THEN** a instalação termina com sucesso e a ferramenta funciona dentro do sandbox

#### Scenario: Host intacto
- **WHEN** um processo confinado instala, altera ou apaga algo numa pasta de toolchain
- **THEN** a pasta correspondente no host continua idêntica

#### Scenario: Persistência entre execuções
- **WHEN** uma ferramenta foi instalada numa execução e o mesmo harness é lançado de novo
- **THEN** a ferramenta continua disponível

#### Scenario: Atualização da toolchain no host
- **WHEN** o usuário instala uma versão nova de uma toolchain no host, fora do sandbox
- **THEN** a execução seguinte enxerga a versão nova

### Requirement: Caches de pacotes com camada privada
Os caches conhecidos MUST ser lidos do host e escritos na camada privada do harness: `~/.npm`, `~/.cache/pip`, `~/.cache/uv`, `~/.cache/go-build`, `~/.cache/ms-playwright` e `~/.cache/huggingface`, além dos que ficam dentro das pastas de toolchain. Os arquivos `token` e `stored_tokens` de `~/.cache/huggingface` MUST aparecer vazios.

#### Scenario: Pacote já baixado no host
- **WHEN** um processo confinado instala um pacote que já está no cache do host
- **THEN** o pacote não é baixado de novo

#### Scenario: Cache do host intacto
- **WHEN** um processo confinado baixa um pacote novo ou altera um arquivo do cache
- **THEN** o cache do host continua idêntico

#### Scenario: Token do Hugging Face
- **WHEN** um processo confinado lê `~/.cache/huggingface/token`
- **THEN** o arquivo aparece vazio

### Requirement: Arquivos de credencial cobertos não podem ser descobertos
Um arquivo ou pasta de credencial que a lista manda cobrir MUST aparecer vazio e MUST NOT poder ser removido, renomeado ou substituído pelo processo confinado para alcançar o conteúdo real.

#### Scenario: Tentativa de contornar a cobertura
- **WHEN** um processo confinado tenta apagar ou renomear um arquivo de credencial coberto
- **THEN** a operação falha e o conteúdo real continua inacessível

### Requirement: Execuções simultâneas do mesmo harness
Quando um harness é lançado enquanto outra execução dele já usa a camada privada de escrita, a nova execução MUST receber uma camada temporária, descartada ao terminar, e o `run` MUST avisar que o que for instalado ou baixado nela não será guardado. O HOME sintético continua compartilhado.

#### Scenario: Segunda execução
- **WHEN** o usuário lança o opencode numa pasta enquanto outro opencode confinado está rodando
- **THEN** a segunda execução inicia, com o aviso sobre a camada temporária

### Requirement: Recuo quando a camada de escrita não funciona
Quando a camada privada de escrita não pode ser montada na máquina, o `run` MUST lançar o sandbox com os caches conhecidos vazios e próprios do harness e com as toolchains em somente leitura, e MUST avisar. Uma pasta do host MUST NOT ser entregue com escrita como alternativa.

#### Scenario: Overlay indisponível
- **WHEN** a camada privada de escrita não pode ser montada
- **THEN** o sandbox é lançado com o aviso, as toolchains funcionam para leitura e uma instalação global falha sem alterar o host

### Requirement: Proteção dos hooks e da configuração do git
Na pasta de trabalho, `.git/hooks` e `.git/config` MUST aparecer em somente leitura, assim como os hooks e a configuração do diretório git de cada submódulo. As demais operações do git MUST continuar funcionando.

#### Scenario: Commit
- **WHEN** um processo confinado cria uma branch, faz um commit, um merge ou um stash
- **THEN** a operação funciona e fica visível no host

#### Scenario: Hook plantado
- **WHEN** um processo confinado tenta criar ou alterar um arquivo em `.git/hooks`
- **THEN** a operação falha

#### Scenario: Configuração alterada
- **WHEN** um processo confinado roda `git config` local ou `git remote add`
- **THEN** a operação falha e `.git/config` continua idêntico

#### Scenario: Submódulo
- **WHEN** a pasta de trabalho tem um submódulo e um processo confinado tenta criar um hook no diretório git dele
- **THEN** a operação falha

#### Scenario: Pasta sem repositório
- **WHEN** a pasta de trabalho não é um repositório git
- **THEN** o lançamento prossegue sem aviso

### Requirement: Pasta de trabalho que é um worktree do git
Quando a pasta de trabalho é um worktree do git, o `run` MUST tornar o diretório git do repositório principal visível no caminho original, com os hooks e a configuração em somente leitura, para que o git funcione dentro do sandbox.

#### Scenario: Commit num worktree
- **WHEN** a pasta de trabalho é um worktree e um processo confinado faz um commit
- **THEN** o commit funciona e aparece no repositório principal

#### Scenario: Hook no repositório principal
- **WHEN** um processo confinado, lançado num worktree, tenta criar um hook no diretório git do repositório principal
- **THEN** a operação falha

### Requirement: Pasta de trabalho que é subpasta de um repositório
Quando o diretório `.git` fica acima da pasta de trabalho, o `run` MUST lançar o sandbox sem o repositório e avisar que o git não funciona lá dentro. Com a opção `--repo`, o `run` MUST montar o repositório inteiro em somente leitura, com a pasta de trabalho gravável e o `.git` gravável exceto hooks e configuração.

#### Scenario: Padrão
- **WHEN** o usuário lança o sandbox numa subpasta de um repositório, sem opções
- **THEN** o aviso é mostrado, o resto do repositório não existe dentro do sandbox e `git status` falha

#### Scenario: Repositório montado
- **WHEN** o usuário lança o sandbox numa subpasta com `--repo`
- **THEN** o git funciona, um commit de arquivos da subpasta funciona e a escrita fora da subpasta falha

### Requirement: Pastas acrescentadas pelo usuário
`sandmux run` MUST aceitar `--ro <caminho>` e `--rw <caminho>`, repetíveis, que acrescentam um arquivo ou pasta do host à visão do sandbox, no caminho original, em somente leitura ou com leitura e escrita. Um caminho que não existe MUST fazer o `run` terminar com erro.

#### Scenario: Pasta somente leitura
- **WHEN** o usuário passa `--ro ~/docs`
- **THEN** o processo confinado lê `~/docs` e não consegue escrever nela

#### Scenario: Pasta com escrita
- **WHEN** o usuário passa `--rw ~/saida`
- **THEN** o processo confinado cria arquivos em `~/saida` e eles aparecem no host

#### Scenario: Caminho inexistente
- **WHEN** o caminho passado não existe
- **THEN** o `run` termina com erro e não lança o sandbox

### Requirement: Aviso ao montar uma credencial conhecida
Quando um caminho de `--ro` ou `--rw` é uma credencial conhecida, está dentro de uma ou contém uma, o `run` MUST avisar a cada execução que a credencial real fica ao alcance do agente, e prosseguir com o lançamento.

#### Scenario: CLI de cloud
- **WHEN** o usuário passa `--ro ~/.kube`
- **THEN** o aviso é mostrado e `kubectl`, se visível no sandbox, usa a credencial

#### Scenario: Pasta que contém uma credencial
- **WHEN** o usuário passa `--ro ~/.config` e existe `~/.config/gh`
- **THEN** o aviso é mostrado, citando a credencial encontrada

#### Scenario: Pasta comum
- **WHEN** o usuário passa `--ro ~/docs`
- **THEN** nenhum aviso de credencial é mostrado

### Requirement: Lista de credenciais conhecidas
A lista de credenciais conhecidas, usada para o aviso de montagem, MUST conter: `~/.ssh`, `~/.gnupg`, `~/.aws`, `~/.kube`, `~/.config/gcloud`, `~/.azure`, `~/.docker`, `~/.config/gh`, `~/.terraform.d`, `~/.netrc`, `~/.npmrc`, `~/.pypirc`, `~/.git-credentials`, `~/.config/git/credentials`, `~/.password-store`, `~/.local/share/keyrings`, `~/.vault-token`, os perfis de navegador e o estado real dos harnesses suportados.

#### Scenario: Estado real de um harness
- **WHEN** o usuário passa `--ro ~/.hermes`
- **THEN** o aviso de credencial é mostrado

#### Scenario: Perfil de navegador
- **WHEN** o usuário passa `--ro ~/.mozilla`
- **THEN** o aviso de credencial é mostrado

### Requirement: Pasta de entrada
O sandmux MUST manter uma pasta de entrada no seu diretório de dados, visível em somente leitura em todo sandbox no caminho `~/inbox`, e informar esse caminho ao processo confinado pela variável `SANDMUX_INBOX`. `sandmux doctor` MUST mostrar o caminho da pasta no host.

#### Scenario: Arquivo entregue ao agente
- **WHEN** o usuário copia um PDF para a pasta de entrada no host
- **THEN** um processo confinado lê o PDF em `~/inbox`, sem reiniciar o sandbox

#### Scenario: Escrita na pasta de entrada
- **WHEN** um processo confinado tenta criar, alterar ou apagar um arquivo em `~/inbox`
- **THEN** a operação falha

#### Scenario: Mesma pasta em todos os harnesses
- **WHEN** o opencode e o hermes rodam confinados
- **THEN** os dois veem o mesmo conteúdo em `~/inbox`

### Requirement: Identidade mínima do git
Ao criar um HOME sintético, o sandmux MUST gravar nele uma configuração global do git contendo apenas `user.name` e `user.email` copiados da configuração do usuário. Depois de criado, o arquivo MUST NOT ser regravado pelo sandmux.

#### Scenario: Autor do commit
- **WHEN** um processo confinado faz um commit
- **THEN** o autor é o nome e o e-mail do usuário

#### Scenario: Configuração sensível do git
- **WHEN** a configuração real do git tem aliases, credential helper ou URL com token
- **THEN** nada disso existe na configuração do HOME sintético

#### Scenario: Usuário sem identidade no git
- **WHEN** a configuração real do git não define nome ou e-mail
- **THEN** o HOME sintético é criado sem o valor que falta e o `run` avisa que o commit vai pedir a identidade

#### Scenario: Ajuste feito pelo usuário
- **WHEN** o usuário edita a configuração do git no HOME sintético e lança o harness de novo
- **THEN** o ajuste continua lá
