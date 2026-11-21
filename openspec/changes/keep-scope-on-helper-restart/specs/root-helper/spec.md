## MODIFIED Requirements

### Requirement: Escopo de um sandbox
O pedido `scope` MUST receber o user namespace de um sandbox, criado por quem pede, e MUST negar aos processos desse namespace e dos namespaces abaixo dele: a conexão a um socket abstrato criado fora do sandbox, um sinal a um processo de fora do sandbox e um `ioctl` num dispositivo fora da lista do pedido, com as mesmas exceções do Landlock. O escopo MUST valer até a conexão do pedido fechar. São até 256 escopos ativos por usuário.

Quando o auxiliar para com sandboxes registrados, o escopo de cada um MUST ser registrado de novo no auxiliar seguinte. Numa sessão do daemon, a aba MUST ficar congelada até o escopo voltar, e nada dela roda sem ele; no `run` sem o daemon, o lançador MUST registrar de novo assim que o auxiliar seguinte responder. Sem o escopo de volta em 10 s, o sandbox MUST terminar. Numa saída pedida (`SIGTERM`), o auxiliar MUST avisar cada conexão antes de soltar as regras dela e MUST NOT atender conexões novas; um lançamento que encontra o auxiliar trocando MUST tentar o registro de novo por até 10 s.

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

#### Scenario: Auxiliar reiniciado com uma aba rodando
- **WHEN** o auxiliar recebe `SIGTERM` com uma aba do daemon registrada, e o auxiliar seguinte sobe
- **THEN** a aba fica congelada até o escopo ser registrado no auxiliar seguinte, e depois segue com os sinais para fora e os sockets abstratos de fora ainda negados

#### Scenario: Auxiliar que não volta
- **WHEN** o auxiliar para e nenhum outro responde em 10 s
- **THEN** o sandbox termina, e nenhum processo dele roda sem o escopo depois desse prazo

#### Scenario: Lançamento durante a troca do auxiliar
- **WHEN** uma aba é lançada enquanto o auxiliar sai e o seguinte sobe
- **THEN** o lançamento registra o escopo no auxiliar seguinte e sobe
