# Spec Delta

## MODIFIED Requirements

### Requirement: Chaves de ajuste
Cada seção MUST aceitar só as chaves do cenário "Chaves aceitas", cada uma com o mesmo efeito da opção correspondente do `run`. As chaves `net-log-days` (número de dias) e `net-log-max` (tamanho) MUST ser aceitas só em `[global]`.

#### Scenario: Chaves aceitas
- **WHEN** o usuário escreve uma seção com cada chave aceita
- **THEN** o arquivo vale com `ro` e `rw` (caminhos absolutos ou iniciados por `~/`, como `--ro` e `--rw`), `pass-env` (nomes de variáveis, como `--pass-env`), `repo`, `kvm`, `gpu` e `clipboard` (verdadeiro ou falso, como `--repo`, `--kvm`, `--no-gpu` quando `gpu` é falso e `--allow-clipboard`), `memory` (porcentagem, tamanho ou `"max"`, como `--memory`), `processes` (número ou `"max"`, como `--processes`), `network` (o modo, como `--network`), `deny` e `allow` (padrões de destino, como `--deny` e `--allow`), `allow-off` e `catalog-off` (padrões desligados), `catalog` (grupos do catálogo de domínios comuns), `decrypt` (padrões de destino decifrados, como `--decrypt`) `ssh-keys` (nomes das chaves SSH do cofre, ou `"*"` para todas, como `--ssh-key`), `file-deny` e `file-allow` (padrões de arquivo, como `--file-deny` e `--file-allow`), `secret-files` e `secret-files-off` (padrões somados e desligados da lista de segredos), `mcp-deny` e `mcp-allow` (nomes de servidores MCP, como `--mcp-deny` e `--mcp-allow`)

#### Scenario: Pasta extra no perfil
- **WHEN** o perfil do projeto tem `ro = ["~/docs"]` e o usuário roda `sandmux run shell` na pasta do projeto
- **THEN** o processo confinado lê `~/docs` e não consegue escrever nela

#### Scenario: Chave desconhecida
- **WHEN** a seção `[global]` tem `rede = "aberta"`
- **THEN** o arquivo é considerado inválido

#### Scenario: Caminho relativo
- **WHEN** a seção `[global]` tem `rw = ["saida"]`
- **THEN** o arquivo é considerado inválido

#### Scenario: Teto inválido
- **WHEN** a seção `[global]` tem `memory = "muito"` ou `processes = 0`
- **THEN** o arquivo é considerado inválido, com a linha e os formatos aceitos na mensagem

#### Scenario: Chave SSH no perfil
- **WHEN** o perfil do projeto tem `ssh-keys = ["github"]` e o usuário roda `sandmux run shell` na pasta do projeto
- **THEN** a sessão tem o agente SSH com a chave `github`

#### Scenario: Padrão de arquivo inválido
- **WHEN** a seção `[global]` tem `file-deny = [""]`
- **THEN** o arquivo é considerado inválido

#### Scenario: Retenção fora do global
- **WHEN** um perfil de projeto tem `net-log-days = 7`
- **THEN** o arquivo é considerado inválido
