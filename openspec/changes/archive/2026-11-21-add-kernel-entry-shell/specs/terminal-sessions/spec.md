## MODIFIED Requirements

### Requirement: Shell dentro do sandbox de uma aba
`sandmux enter [id]` e Ctrl+b `t` na TUI MUST abrir uma sessão nova com um shell no sandbox da aba alvo viva, com:
- a visão de arquivos, a rede, o Landlock e o seccomp do comando;
- as aberturas pelo caminho do comando (o kernel ou o supervisor);
- o mesmo pid namespace.

Sem `id`, o alvo MUST ser a única sessão viva. Uma aba de dentro MUST recusar abrir outra, e uma aba alvo que não está viva MUST ser recusada com o motivo. Após um reinício do daemon, a aba de dentro MUST voltar parada.

#### Scenario: O que o agente vê
- **WHEN** a aba 1 roda um processo `sleep 4321` e tem um `.env` na pasta de trabalho, e o usuário roda `sandmux enter 1`
- **THEN** o shell está na mesma pasta, enxerga o `sleep 4321`, não lê o `.env` e não escreve em `/etc`

#### Scenario: Pela TUI
- **WHEN** na aba `shell:app` o usuário aperta Ctrl+b `t`
- **THEN** abre a aba `dentro:app`, com o shell dentro do sandbox de `shell:app`

#### Scenario: Aba confinada pelo kernel
- **WHEN** a aba 1 é confinada pelo kernel, com um `.env` e uma pasta em `file-deny`, e o usuário roda `sandmux enter 1` e lê esses arquivos e um arquivo comum
- **THEN** o shell não lê o `.env` nem a pasta negada, a leitura do arquivo comum entra no registro pela fila do kernel, e o shell tem os mesmos filtros seccomp que o comando da aba
