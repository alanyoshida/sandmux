# Spec Delta

## MODIFIED Requirements

### Requirement: Chaves de ajuste
Cada seção MUST aceitar só estas chaves: `ro` e `rw`, listas de caminhos absolutos ou iniciados por `~/`; `pass-env`, lista de nomes de variáveis; `repo`, `kvm`, `gpu` e `clipboard`, valores verdadeiro ou falso; `memory`, texto com porcentagem, tamanho ou `"max"`; e `processes`, número ou `"max"`. Cada chave MUST ter o mesmo efeito da opção correspondente do `run`: `--ro`, `--rw`, `--pass-env`, `--repo`, `--kvm`, `--no-gpu` (quando `gpu` é falso), `--allow-clipboard`, `--memory` e `--processes`.

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

### Requirement: Precedência nas chaves de liga e desliga
Nas chaves `repo`, `kvm`, `gpu`, `clipboard`, `memory` e `processes`, MUST valer o valor da camada mais específica que definiu a chave.

#### Scenario: Projeto afrouxa o global
- **WHEN** `[global]` tem `gpu = false` e o perfil do projeto tem `gpu = true`
- **THEN** o sandbox lançado na pasta do projeto enxerga a GPU

#### Scenario: Opção do run vence o perfil
- **WHEN** o perfil do projeto tem `kvm = true` e o usuário roda `sandmux run shell --no-kvm` na pasta do projeto
- **THEN** `/dev/kvm` não existe dentro do sandbox

#### Scenario: Teto do projeto vence o global
- **WHEN** `[global]` tem `memory = "8G"` e o perfil do projeto tem `memory = "max"`
- **THEN** o sandbox lançado na pasta do projeto não tem teto de memória
