# Spec Delta

## ADDED Requirements

### Requirement: Portas no painel
O painel lateral MUST ter uma seção "Portas" com as portas publicadas da aba ativa (porta do sandbox e porta do host) e as publicadas pelos contêineres do Docker privado da pasta, atualizada a cada segundo.

#### Scenario: Porta nova
- **WHEN** o agente da aba ativa passa a escutar na porta 3000
- **THEN** a seção Portas mostra `3000 → 127.0.0.1:3000` em até 2 segundos
