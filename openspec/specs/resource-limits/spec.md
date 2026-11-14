# resource-limits Specification

## Purpose
Define os tetos de memória e de processos de cada sandbox: os valores padrão, como são ajustados, o que acontece ao estourar, o comportamento numa máquina sem suporte, a proteção dos limites contra o agente e como o consumo de cada sessão é mostrado.

## Requirements

### Requirement: Teto padrão de memória
Todo lançamento MUST ter um teto de memória de 75% da RAM total da máquina, calculado no lançamento, sem uso de swap pelo sandbox. Um teto em porcentagem MUST ser convertido pela RAM total da máquina naquele lançamento.

#### Scenario: Sem configuração
- **WHEN** a máquina tem 46 GB de RAM e o usuário roda `sandmux run shell` sem configuração
- **THEN** o teto de memória do sandbox é 75% de 46 GB, e o sandbox não usa swap

### Requirement: Teto padrão de processos
Todo lançamento MUST ter um teto de 8192 processos e threads somados.

#### Scenario: Fork bomb
- **WHEN** um processo confinado cria processos sem parar
- **THEN** a criação de processos falha dentro do sandbox depois de 8192, e os processos fora do sandbox continuam criando processos normalmente

### Requirement: Estouro de memória fica dentro do sandbox
Quando o sandbox passa do teto de memória, o kernel MUST matar processos só daquele sandbox. Processos de outras sessões e do host MUST NOT ser afetados.

#### Scenario: Programa que aloca demais
- **WHEN** o teto é `512M` e um programa confinado tenta alocar 2 GB
- **THEN** esse programa é morto por falta de memória, o shell da sessão continua, e as outras sessões não são afetadas

### Requirement: Tetos ajustáveis
Os tetos MUST ser ajustáveis pela chave `memory` e pela opção `--memory`, que aceitam porcentagem da RAM de 1% a 100% (`"50%"`), tamanho com sufixo `K`, `M`, `G` ou `T` (`"16G"`) ou `"max"`, sem teto; e pela chave `processes` e pela opção `--processes`, que aceitam um número ou `"max"`. Teto de memória abaixo de `128M` ou de processos abaixo de 32 MUST ser recusado.

#### Scenario: Teto menor no projeto
- **WHEN** o perfil do projeto tem `memory = "4G"` e `processes = 512`
- **THEN** o sandbox lançado na pasta do projeto tem teto de 4 GB de memória e de 512 processos

#### Scenario: Sem teto
- **WHEN** o usuário roda `sandmux run shell --memory max`
- **THEN** o sandbox não tem teto de memória e mantém o teto de processos

#### Scenario: Valor inválido
- **WHEN** o usuário roda `sandmux run shell --memory 64M`
- **THEN** o `run` termina com erro dizendo o mínimo aceito e não lança o sandbox

### Requirement: Mesmos tetos sem terminal
O `run` sem terminal MUST aplicar os mesmos tetos do `run` com terminal, resolvidos pelas mesmas camadas.

#### Scenario: Script
- **WHEN** um script roda `sandmux run shell --processes 64 -- -c "<cria 100 processos>"`
- **THEN** a criação de processos falha dentro do sandbox depois de 64

### Requirement: Agente não altera os próprios limites
O processo confinado MUST NOT conseguir aumentar os tetos, mover-se para fora do grupo de controle do sandbox nem criar um grupo de controle fora dele.

#### Scenario: Gravar no teto
- **WHEN** um processo confinado tenta gravar em `memory.max` ou `cgroup.procs` do próprio grupo de controle, ou de um grupo acima
- **THEN** a gravação é negada e os tetos continuam os mesmos

### Requirement: Máquina sem suporte a limites
Quando a máquina não permite aplicar os tetos sem privilégio, o lançamento MUST seguir sem tetos e MUST avisar, a cada lançamento, que a sessão roda sem limite de memória e de processos, com a causa. Isso acontece sem systemd de usuário, ou sem os controladores `memory` e `pids` delegados ao usuário.

#### Scenario: Sem systemd de usuário
- **WHEN** o systemd de usuário não responde e o usuário roda `sandmux run shell`
- **THEN** o shell sobe, e o aviso diz que a sessão roda sem limite de memória e de processos e indica `sandmux doctor`

### Requirement: Consumo em sandmux ls
`sandmux ls` MUST mostrar, para cada sessão viva, a memória em uso e o teto, e os processos em uso e o teto. Uma sessão sem teto MUST mostrar o consumo com "sem teto". Uma sessão encerrada ou parada MUST mostrar o consumo como vazio.

#### Scenario: Sessão viva
- **WHEN** a sessão 1 está viva com 1,2 GB em uso, 37 processos e os tetos padrão numa máquina de 46 GB
- **THEN** a linha da sessão 1 mostra a memória em uso e o teto de 34,5 GB, e `37/8192` na coluna de processos

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

### Requirement: Sessão encerrada por falta de memória
Quando o harness de uma sessão termina depois de pelo menos uma morte por falta de memória no lançamento, `sandmux ls` MUST mostrar o estado como encerrada com o código e a nota "memória esgotada".

#### Scenario: Harness morto pelo teto
- **WHEN** o harness da sessão 2 passa do teto de memória, é morto e a sessão termina com código 137
- **THEN** `sandmux ls` mostra a sessão 2 como "encerrada (137, memória esgotada)"
