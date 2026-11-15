# harness-presets Specification

## Purpose

Define o que cada harness suportado precisa para rodar confinado pelo sandmux: como ele é localizado na máquina, o que do seu código e do seu estado entra no sandbox, que ambiente recebe e o que acontece quando ele não está instalado.

## Requirements

### Requirement: Harnesses suportados
`sandmux run <harness>` MUST aceitar os presets embutidos `opencode`, `hermes` e `shell`. Um nome que não corresponde a um preset MUST fazer o comando terminar com erro, listando os presets disponíveis, sem lançar nada.

#### Scenario: Preset conhecido
- **WHEN** o usuário roda `sandmux run opencode`
- **THEN** o opencode é lançado confinado na pasta de trabalho

#### Scenario: Preset desconhecido
- **WHEN** o usuário roda `sandmux run` com um nome que não é um preset
- **THEN** o comando termina com erro e lista `opencode`, `hermes` e `shell`

### Requirement: Lançamento sem configuração
Um preset MUST lançar o harness sem que o usuário escreva configuração nem passe opções: a baseline de segurança, as montagens do harness e o ambiente vêm do próprio sandmux.

#### Scenario: Primeira execução sem opções
- **WHEN** o usuário roda `sandmux run opencode` numa máquina com suporte, sem arquivo de configuração do sandmux
- **THEN** o harness inicia confinado

### Requirement: Harness não instalado
Quando o harness pedido não está instalado na máquina, `sandmux run` MUST terminar com erro, dizer onde procurou e mostrar como instalar o harness no host. O sandmux MUST NOT instalar um harness.

#### Scenario: opencode ausente
- **WHEN** o usuário roda `sandmux run opencode` e `~/.opencode/bin/opencode` não existe
- **THEN** o comando termina com erro, informa o caminho procurado e a instrução de instalação, e não lança o sandbox

#### Scenario: hermes ausente
- **WHEN** o usuário roda `sandmux run hermes` e a instalação em `~/.hermes/hermes-agent` não existe
- **THEN** o comando termina com erro com a instrução de instalação

### Requirement: Código do harness em somente leitura
O código do harness MUST entrar no sandbox em somente leitura, no caminho original. Um processo confinado MUST NOT conseguir alterar o código do harness que o usuário executa, nem dentro nem fora do sandbox.

#### Scenario: Tentativa de alterar o harness
- **WHEN** um processo confinado tenta alterar ou substituir um arquivo do código do harness
- **THEN** a operação falha

#### Scenario: Atualização feita no host
- **WHEN** o usuário atualiza o harness no host e lança o sandbox de novo
- **THEN** a versão nova é a que roda confinada

### Requirement: Checagem de atualização desligada
O preset MUST desligar a checagem e a atualização automática do harness dentro do sandbox: no opencode, pela variável `OPENCODE_DISABLE_AUTOUPDATE`; no hermes, pela chave `updates.check: false` no `config.yaml` do HOME sintético, garantida a cada lançamento e a cada importação.

#### Scenario: opencode
- **WHEN** o opencode confinado inicia
- **THEN** ele não tenta se atualizar

#### Scenario: hermes após importação
- **WHEN** a configuração importada do hermes tinha a checagem ligada
- **THEN** o `config.yaml` do HOME sintético fica com `updates.check: false`

### Requirement: Preset do opencode
O preset do opencode MUST executar o binário de `~/.opencode/bin`, com o código em `~/.opencode` em somente leitura, e manter a configuração, os dados, o estado e o cache do opencode no HOME sintético.

#### Scenario: Início
- **WHEN** o usuário roda `sandmux run opencode` numa pasta de projeto
- **THEN** a interface do opencode abre com a pasta de trabalho como projeto

#### Scenario: Sessões do opencode
- **WHEN** o usuário conversa com o agente e encerra
- **THEN** a sessão fica guardada no HOME sintético e não no estado real do opencode

#### Scenario: Modelos de um servidor local
- **WHEN** a configuração do opencode tem um provedor com `baseURL` `http://127.0.0.1:8888/v1`, a porta 8888 está em `host-ports`, e o servidor responde a `GET /v1/models` com `Bonsai-27B-GGUF`
- **THEN** a sessão do opencode lançada pelo daemon tem `Bonsai-27B-GGUF` entre os modelos desse provedor, sem editar o arquivo de configuração, e a consulta usou o valor real da chave do cofre

#### Scenario: Servidor local fora do ar
- **WHEN** o servidor do provedor local não responde
- **THEN** a sessão sobe com os modelos da configuração, sem erro

### Requirement: Preset do hermes
O preset do hermes MUST executar o hermes instalado em `~/.hermes/hermes-agent`, com o código e os comandos `hermes`, `hermes-agent` e `hermes-acp` em somente leitura, e manter todo o restante de `~/.hermes` no HOME sintético. As ferramentas que o hermes baixou em `~/.hermes/tools` e os ambientes de dependências em `~/.hermes/installs` MUST ser lidos do host e escritos na camada privada do harness.

#### Scenario: Início
- **WHEN** o usuário roda `sandmux run hermes` numa pasta de projeto
- **THEN** a interface do hermes abre

#### Scenario: Estado real do hermes
- **WHEN** um processo confinado lê `~/.hermes/.env` ou `~/.hermes/auth.json`
- **THEN** ele lê os arquivos do HOME sintético, e não os reais

### Requirement: Gateway próprio do hermes
O hermes confinado MUST usar um gateway próprio, iniciado dentro do sandbox. O socket e os arquivos de estado do gateway do host MUST NOT existir dentro do sandbox.

#### Scenario: Gateway do host rodando
- **WHEN** o usuário já tem um gateway do hermes rodando no host e lança o hermes confinado
- **THEN** o hermes confinado não se conecta ao gateway do host

#### Scenario: Fim da execução
- **WHEN** o hermes confinado termina
- **THEN** o gateway que ele iniciou também termina

### Requirement: Backend Docker do hermes não suportado
O backend de execução em Docker do hermes MUST NOT funcionar dentro do sandbox nesta fatia, porque nenhum socket de Docker existe lá dentro.

#### Scenario: Backend Docker configurado
- **WHEN** a configuração do hermes usa o backend Docker e o agente tenta executar um comando por ele
- **THEN** a execução falha por falta de acesso ao Docker, sem alcançar o Docker do host

### Requirement: Preset shell
O preset `shell` MUST lançar um shell interativo confinado, sem harness, com HOME sintético próprio. Ele MUST usar o shell do usuário quando esse shell está nas pastas de sistema, e o `bash` caso contrário, com um arquivo de configuração mínimo gerado ao criar o HOME sintético. Os arquivos de configuração de shell do HOME real MUST NOT ser copiados.

#### Scenario: Início
- **WHEN** o usuário roda `sandmux run shell`
- **THEN** um shell interativo abre na pasta de trabalho, com as toolchains conhecidas no `PATH`

#### Scenario: Configuração real do shell
- **WHEN** o `.bashrc` real do usuário exporta um token
- **THEN** o token não existe dentro do shell confinado

#### Scenario: Shell fora do sistema
- **WHEN** o shell do usuário está instalado dentro do HOME
- **THEN** o preset usa o `bash`

#### Scenario: Ajuste do usuário
- **WHEN** o usuário edita o arquivo de configuração do shell no HOME sintético
- **THEN** o ajuste vale nas execuções seguintes

### Requirement: Argumentos repassados ao harness
Os argumentos escritos depois de `--` em `sandmux run <harness> -- <argumentos>` MUST ser repassados ao harness sem alteração.

#### Scenario: Comando do harness
- **WHEN** o usuário roda `sandmux run opencode -- --version`
- **THEN** o opencode confinado recebe `--version` e mostra a versão

#### Scenario: Comando no preset shell
- **WHEN** o usuário roda `sandmux run shell -- -c "id -u"`
- **THEN** o shell confinado executa o comando e o `run` termina com o código de saída dele

### Requirement: Chave de API enquanto o cofre não existe
Com `--pass-env`, o harness MUST conseguir chamar o modelo usando a chave passada pelo usuário, mesmo quando a importação deixou um valor falso no arquivo de credenciais do harness.

#### Scenario: opencode com chave passada
- **WHEN** o usuário importou o opencode e roda `sandmux run opencode --pass-env OPENROUTER_API_KEY`
- **THEN** o opencode responde a um prompt usando o modelo do OpenRouter

#### Scenario: hermes com chave passada
- **WHEN** o usuário importou o hermes e roda `sandmux run hermes --pass-env OPENROUTER_API_KEY`
- **THEN** o hermes enxerga, em `~/.hermes/.env`, um arquivo só daquela execução, em somente leitura, com o valor real de `OPENROUTER_API_KEY` no lugar do falso e as outras linhas iguais às do `.env` do HOME sintético
- **AND** o `.env` do HOME sintético, no host, continua com o valor falso depois da execução

#### Scenario: Valor que o .env do hermes não representa
- **WHEN** o usuário roda `sandmux run hermes --pass-env NOME` e o valor de `NOME` tem quebra de linha ou o trecho `${`
- **THEN** o `run` recusa antes de criar o sandbox, com uma mensagem que cita o nome da variável e não mostra o valor

#### Scenario: Sem chave
- **WHEN** o usuário roda um harness sem `--pass-env` depois de importar
- **THEN** o harness inicia e a chamada ao modelo falha por credencial inválida, sem que nenhuma chave real exista no sandbox

### Requirement: Preset do Claude Code
O preset `claude` MUST rodar o Claude Code instalado no host (`~/.local/bin/claude`, com a instalação em `~/.local/share/claude`) em somente leitura, com a atualização automática desligada. Sem a instalação, o `run` MUST terminar com a dica de instalar. A importação MUST copiar `~/.claude.json` e as configurações, agentes, comandos, skills, hooks e plugins de `~/.claude`, e MUST deixar de fora `~/.claude/.credentials.json`, as conversas e os caches.

#### Scenario: Importação sem o token do host
- **WHEN** o usuário importa o Claude Code, e `~/.claude/.credentials.json` existe
- **THEN** o HOME sintético tem `settings.json` e `.claude.json`, e não tem `.credentials.json`

### Requirement: Preset do oh-my-pi
O preset `omp` MUST rodar o binário `~/.local/bin/omp` do host em somente leitura e MUST receber as chaves falsas dos provedores com segredo no cofre por variável de ambiente. Sem o binário, o `run` MUST terminar com a dica de instalar. A importação MUST copiar o `config.yml` e as pastas de extensões, agentes, regras e skills de `~/.omp/agent`, e MUST deixar de fora o `agent.db` (credenciais), os outros bancos, as sessões e os caches.

#### Scenario: Importação sem o banco de credenciais
- **WHEN** o usuário importa o oh-my-pi, e `~/.omp/agent/agent.db` existe
- **THEN** o HOME sintético tem `~/.omp/agent/config.yml` e as extensões, e não tem `agent.db`
