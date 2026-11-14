## MODIFIED Requirements

### Requirement: Seções do arquivo
O arquivo MUST aceitar cinco tipos de seção: `[global]`, que vale para toda execução; `[profile.<nome>]`, um perfil nomeado; `[project."<pasta>"]`, o perfil de um projeto, ligado ao caminho absoluto de uma pasta; `[tui]`, com a tecla de prefixo e os atalhos da interface; e `[assistant]`, com os modelos do assistente (`chat-model` e `models`) e, em `[assistant.provider.<nome>]`, a `url` de cada provedor próprio. `[tui]` e `[assistant]` não são camadas de política. Nenhuma outra seção MUST ser aceita.

#### Scenario: As três seções
- **WHEN** o arquivo tem `[global]`, `[profile.web]` e `[project."/home/ana/proj"]`
- **THEN** o arquivo é aceito e cada seção é uma camada própria

#### Scenario: Seção desconhecida
- **WHEN** o arquivo tem uma seção `[rede]`
- **THEN** o arquivo é considerado inválido

#### Scenario: Projeto com caminho relativo
- **WHEN** o arquivo tem `[project."proj"]`
- **THEN** o arquivo é considerado inválido

#### Scenario: Seção da interface
- **WHEN** o arquivo tem `[tui]` com `prefix = "C-a"` e uma chave desconhecida `cor = 1`
- **THEN** o arquivo é considerado inválido, citando a chave `cor` e a linha

#### Scenario: Seção do assistente
- **WHEN** o arquivo tem `[assistant]` com `chat-model = "openrouter/x"` e `[assistant.provider.local]` com `url = "http://host.sandmux.internal:11434/v1"`
- **THEN** o arquivo é aceito, e uma chave desconhecida em `[assistant]` o torna inválido, citando a chave e a linha
