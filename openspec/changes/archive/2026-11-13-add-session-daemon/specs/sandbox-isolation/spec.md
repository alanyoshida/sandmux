# Spec Delta

## MODIFIED Requirements

### Requirement: Terminal do usuário entregue ao comando
O comando confinado MUST rodar com entrada, saída e redimensionamento funcionando como numa execução direta. Com terminal interativo, o `run` MUST entregar ao comando o terminal de uma sessão do daemon, anexada ao terminal de onde o `run` foi chamado. Sem terminal interativo, o comando confinado MUST rodar em primeiro plano, direto, com a entrada e a saída de onde o `run` foi chamado, sem daemon.

#### Scenario: Interface de terminal do harness
- **WHEN** um harness com interface de tela cheia é lançado
- **THEN** ele desenha a interface, recebe teclado e mouse, e acompanha o redimensionamento da janela

#### Scenario: Run com terminal
- **WHEN** o usuário roda `sandmux run shell` num terminal
- **THEN** o shell confinado roda numa sessão do daemon, anexada àquele terminal

#### Scenario: Run sem terminal
- **WHEN** um script roda `sandmux run shell -- -c "echo ok"` com a saída num pipe
- **THEN** o pipe recebe exatamente `ok` e uma quebra de linha, sem sequências de terminal, e nenhuma sessão é criada no daemon

### Requirement: Ciclo de vida e código de saída
O `run` MUST terminar com o mesmo código de saída do comando confinado quando o comando termina enquanto o `run` está ligado a ele. Sem terminal, o `run` MUST repassar ao comando os sinais de término recebidos. Com terminal, um sinal de término ou o fechamento do terminal MUST só desanexar, e a sessão continua. Quando o comando confinado termina, todos os processos restantes do sandbox MUST ser encerrados. Processos órfãos dentro do sandbox MUST ser recolhidos.

#### Scenario: Código de saída
- **WHEN** o comando confinado termina com código 3
- **THEN** `sandmux run` termina com código 3

#### Scenario: Processos em segundo plano
- **WHEN** o harness termina deixando processos em segundo plano no sandbox
- **THEN** esses processos são encerrados e nenhum continua rodando no host

#### Scenario: Sinal de término
- **WHEN** o `run` sem terminal recebe um sinal de término
- **THEN** o comando confinado recebe o sinal e o sandbox é desfeito

#### Scenario: Sinal de término com terminal
- **WHEN** o `run` com terminal recebe um sinal de término ou o terminal é fechado
- **THEN** o cliente se desanexa e a sessão continua viva no daemon

#### Scenario: Desanexado antes do fim
- **WHEN** o usuário desanexa com Ctrl+b d antes de o harness terminar
- **THEN** `sandmux run` termina com código zero e informa como reanexar

#### Scenario: Processo órfão
- **WHEN** um processo do sandbox termina depois que o pai dele já saiu
- **THEN** ele é recolhido e não permanece como zumbi
