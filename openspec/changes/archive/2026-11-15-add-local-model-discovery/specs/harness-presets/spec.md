## MODIFIED Requirements

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
