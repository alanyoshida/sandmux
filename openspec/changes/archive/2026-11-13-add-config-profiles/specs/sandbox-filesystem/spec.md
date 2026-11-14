# Spec Delta

## MODIFIED Requirements

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
