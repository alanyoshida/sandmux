# Spec Delta

## MODIFIED Requirements

### Requirement: Chaves de ajuste
Cada seção MUST aceitar só as chaves do cenário "Chaves aceitas", cada uma com o mesmo efeito da opção correspondente do `run`. As chaves `net-log-days` (número de dias) e `net-log-max` (tamanho) MUST ser aceitas só em `[global]`.

#### Scenario: Chaves aceitas
- **WHEN** o usuário escreve uma seção com cada chave aceita
- **THEN** o arquivo vale com `ro` e `rw` (caminhos absolutos ou iniciados por `~/`, como `--ro` e `--rw`), `pass-env` (nomes de variáveis, como `--pass-env`), `repo`, `kvm`, `gpu` e `clipboard` (verdadeiro ou falso, como `--repo`, `--kvm`, `--no-gpu` quando `gpu` é falso e `--allow-clipboard`), `memory` (porcentagem, tamanho ou `"max"`, como `--memory`), `processes` (número ou `"max"`, como `--processes`), `network` (o modo, como `--network`), `deny` e `allow` (padrões de destino, como `--deny` e `--allow`), `allow-off` e `catalog-off` (padrões desligados) e `catalog` (grupos do catálogo de domínios comuns)

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

#### Scenario: Retenção fora do global
- **WHEN** um perfil de projeto tem `net-log-days = 7`
- **THEN** o arquivo é considerado inválido
