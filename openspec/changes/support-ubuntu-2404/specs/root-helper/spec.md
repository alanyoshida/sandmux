## MODIFIED Requirements

### Requirement: Pedidos ao auxiliar root
O `sandmux-helper` MUST rodar como serviço do sistema, com root, num socket Unix local que qualquer usuário alcança. Ele MUST aceitar só seis pedidos: a consulta de estado (versão e BPF LSM ativo), o confinamento e a observação de um cgroup v2 que pertence a quem pede, a marcação e a desmarcação de arquivos de um confinamento, e o escopo de um user namespace de quem pede, os cinco últimos com descritores abertos por quem pede. Qualquer outro pedido, ou um cgroup ou namespace de outro usuário, MUST ser recusado com erro, sem efeito.

#### Scenario: cgroup de outro usuário
- **WHEN** um usuário comum pede o confinamento passando o diretório de um cgroup que pertence ao root
- **THEN** o auxiliar recusa o pedido, e nenhum programa é preso àquele cgroup

#### Scenario: Pedido desconhecido
- **WHEN** um cliente manda ao socket uma linha que não é um pedido conhecido
- **THEN** o auxiliar responde com erro e continua atendendo os outros clientes

#### Scenario: Desmarcar
- **WHEN** um confinamento tem `dados.csv` marcado como negado e quem o pediu manda o `unmark` de `dados.csv`
- **THEN** os processos daquele cgroup voltam a abrir `dados.csv`, e o auxiliar solta o descritor do arquivo

## ADDED Requirements

### Requirement: Escopo de um sandbox
O pedido `scope` MUST receber o user namespace de um sandbox, criado por quem pede, e MUST negar aos processos desse namespace e dos namespaces abaixo dele: a conexão a um socket abstrato criado fora do sandbox, um sinal a um processo de fora do sandbox e um `ioctl` num dispositivo fora da lista do pedido, com as mesmas exceções do Landlock. O escopo MUST valer até a conexão do pedido fechar. São até 256 escopos ativos por usuário.

#### Scenario: Socket abstrato do host
- **WHEN** um sandbox registrado tenta se conectar a um socket abstrato criado por um processo do host
- **THEN** a conexão é negada

#### Scenario: Socket abstrato do próprio sandbox
- **WHEN** dois processos de um sandbox registrado se comunicam por um socket abstrato criado lá dentro
- **THEN** a comunicação funciona

#### Scenario: Sinal para fora
- **WHEN** um processo de um sandbox registrado manda um sinal ao supervisor do sandbox, que está fora dele
- **THEN** o sinal é negado

#### Scenario: Namespace de outro usuário
- **WHEN** um usuário comum pede o `scope` passando o user namespace de outro usuário
- **THEN** o auxiliar recusa o pedido, e nada muda para os processos daquele namespace

#### Scenario: Fim do pedido
- **WHEN** a conexão do pedido `scope` fecha
- **THEN** o namespace sai do registro, e os programas são soltos quando não resta nenhum escopo
