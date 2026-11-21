## MODIFIED Requirements

### Requirement: Processos do host fora do alcance
Um processo confinado MUST enxergar apenas os processos do próprio sandbox e MUST NOT conseguir sinalizar, inspecionar ou depurar um processo do host. Isso MUST continuar valendo depois de um reinício do auxiliar root, nos kernels em que o escopo do sandbox vem dele.

#### Scenario: Listagem de processos
- **WHEN** um processo confinado lista os processos pelo `/proc`
- **THEN** só aparecem processos do próprio sandbox

#### Scenario: Sinal para o host
- **WHEN** um processo confinado tenta enviar um sinal a um processo do host
- **THEN** a tentativa falha

#### Scenario: Depuração dentro do sandbox
- **WHEN** um depurador confinado se anexa a outro processo do mesmo sandbox
- **THEN** a operação funciona

#### Scenario: Sinal para o host depois do reinício do auxiliar
- **WHEN** o kernel tem o Landlock abaixo do ABI 6, o auxiliar root reinicia com uma aba rodando, e um processo dela tenta mandar um sinal ao supervisor do sandbox
- **THEN** a tentativa falha
