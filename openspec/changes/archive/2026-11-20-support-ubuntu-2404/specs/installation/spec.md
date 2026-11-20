## ADDED Requirements

### Requirement: Diagnóstico do kernel sem o Landlock ABI 6
Quando o kernel tem o Landlock abaixo do ABI 6, `sandmux doctor` MUST dizer se o sandbox pode subir pelo auxiliar root (o `sandmux-helper` instalado e o `bpf` entre os LSMs ativos) e, quando não pode, MUST mostrar os dois caminhos: um kernel 6.12 ou mais novo (no Ubuntu 24.04, o kernel HWE, com o comando do `apt`) e o auxiliar com o `bpf` no parâmetro `lsm=` do boot, com os comandos. `sandmux setup` MUST NOT mudar os parâmetros do boot.

#### Scenario: Ubuntu 24.04 com o kernel 6.8
- **WHEN** o usuário roda `sandmux doctor` no Ubuntu 24.04 com o kernel 6.8, sem o `bpf` entre os LSMs ativos
- **THEN** o diagnóstico mostra que o sandbox não sobe, recomenda `sudo apt install linux-generic-hwe-24.04` e mostra como pôr o `bpf` no `lsm=` do GRUB e instalar o auxiliar

#### Scenario: Kernel 6.8 pronto
- **WHEN** o kernel tem o Landlock ABI 4, o `bpf` está entre os LSMs ativos e o auxiliar está instalado
- **THEN** o diagnóstico mostra que o sandbox sobe pelo auxiliar root

#### Scenario: Kernel HWE
- **WHEN** o usuário roda `sandmux doctor` no Ubuntu 24.04 com o kernel HWE 6.14
- **THEN** o diagnóstico mostra o Landlock com o escopo de sockets abstratos e de sinais, sem pedir o auxiliar
