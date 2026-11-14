# Spec Delta

## MODIFIED Requirements

### Requirement: Modo de rede nas camadas
A chave `network` e a opção `--network` MUST aceitar `aberta`, `inferencia`, `custom` e `sem-rede`, e seguir a precedência das camadas da configuração (vence a camada mais específica). O padrão MUST ser `aberta`. O que cada modo deixa passar é definido na capacidade `network-policy`.

#### Scenario: Projeto sem rede
- **WHEN** o perfil do projeto tem `network = "sem-rede"` e o usuário roda `sandmux run shell` na pasta do projeto
- **THEN** o sandbox não tem rede para fora

#### Scenario: Modo inválido
- **WHEN** o `[global]` tem `network = "fechada"`
- **THEN** o arquivo é considerado inválido, com os quatro modos na mensagem
