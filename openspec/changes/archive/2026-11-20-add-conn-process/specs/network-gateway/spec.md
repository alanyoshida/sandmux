## ADDED Requirements

### Requirement: Processo de cada conexão
Cada linha do registro de rede de uma conexão de uma aba (TCP ou UDP) MUST ter o pid e o executável do processo que a abriu, quando ele é conhecido, e a fonte (`proc_via`). Com o auxiliar root e o BPF LSM ativos, as conexões TCP MUST ter o processo vindo do kernel (`kernel`), registrado no momento do `connect`. Sem eles, e no UDP, o processo MUST vir do dono do socket no `/proc` do sandbox (`proc`), procurado no começo da conexão. As linhas do Docker privado MUST NOT ter processo.

#### Scenario: curl numa aba
- **WHEN** o confinamento por eBPF vale e o agente roda `curl https://exemplo.com`
- **THEN** a linha da conexão tem o pid do `curl`, o executável `/usr/bin/curl` e `proc_via` = `kernel`

#### Scenario: Sem o auxiliar
- **WHEN** o `sandmux-helper` não está instalado e o agente abre uma conexão que fica aberta por um segundo
- **THEN** a linha da conexão tem o pid e o executável, com `proc_via` = `proc`

#### Scenario: Conexões curtas sem o auxiliar
- **WHEN** o `sandmux-helper` não está instalado e o agente abre em sequência conexões de cerca de 2 ms, cada uma mandando um pedido e esperando a resposta
- **THEN** as linhas delas têm o processo, com `proc_via` = `proc`, porque o gateway procura o socket antes de abrir a conexão de saída

#### Scenario: Docker
- **WHEN** um contêiner do Docker privado abre uma conexão
- **THEN** a linha dela não tem processo

### Requirement: Processo na consulta do registro
`sandmux net` e a tela de rede da TUI MUST mostrar o processo de cada linha que o tem, com o nome do executável e o pid.

#### Scenario: Consultar
- **WHEN** a sessão 1 abriu uma conexão pelo `curl` e o usuário roda `sandmux net 1`
- **THEN** a linha mostra `curl` e o pid
