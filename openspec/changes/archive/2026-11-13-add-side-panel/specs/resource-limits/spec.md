# Spec Delta

## MODIFIED Requirements

### Requirement: Detalhe do consumo
`sandmux stats <id>` MUST mostrar, para uma sessão viva, a memória em uso, o pico e o teto; os processos em uso, o pico e o teto; o tempo de CPU usado; quantos processos foram mortos por falta de memória; e quantas criações de processo foram recusadas pelo teto. Quando o Docker privado da pasta da sessão está rodando, MUST mostrar também a memória e os processos dele, com os tetos, e quantos contêineres estão rodando. Uma sessão que não está viva MUST mostrar o estado e as contagens de mortes por falta de memória do último lançamento. O comando MUST mostrar também o resumo da proteção do último lançamento, como no painel lateral da TUI.

#### Scenario: Depois de uma morte por falta de memória
- **WHEN** um programa da sessão 1 foi morto por passar do teto de memória e a sessão continua viva
- **THEN** `sandmux stats 1` mostra uma morte por falta de memória e o pico de memória perto do teto

#### Scenario: Sessão inexistente
- **WHEN** o usuário roda `sandmux stats 9` e não existe a sessão 9
- **THEN** o comando termina com erro listando as sessões

#### Scenario: Docker privado rodando
- **WHEN** a sessão 1 tem dois contêineres rodando no Docker privado da pasta
- **THEN** `sandmux stats 1` mostra a seção do Docker privado com dois contêineres, a memória e os processos dele

#### Scenario: Resumo da proteção
- **WHEN** a sessão 1 foi lançada com `--kvm`
- **THEN** `sandmux stats 1` mostra o KVM liberado no resumo da proteção
