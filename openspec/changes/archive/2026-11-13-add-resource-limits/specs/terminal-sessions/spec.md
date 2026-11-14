# Spec Delta

## MODIFIED Requirements

### Requirement: Listar sessões
`sandmux ls` MUST listar as sessões do usuário com o identificador, o nome, o harness, a pasta de trabalho, o estado (viva, encerrada com o código de saída, ou parada com o motivo), quantos clientes estão anexados, e a memória e os processos em uso com os tetos. Sem sessões, o comando MUST dizer isso e terminar com código zero.

#### Scenario: Três estados
- **WHEN** existe uma sessão viva, uma encerrada com código 2 e uma parada por falta de variável
- **THEN** `sandmux ls` mostra as três, com os estados "viva", "encerrada (2)" e "parada" com o motivo

#### Scenario: Consumo
- **WHEN** existe uma sessão viva e uma encerrada
- **THEN** a linha da viva mostra a memória e os processos em uso, cada um com o teto, e a da encerrada mostra o consumo vazio
