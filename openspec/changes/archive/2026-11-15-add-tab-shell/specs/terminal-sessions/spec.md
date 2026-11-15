## ADDED Requirements

### Requirement: Shell dentro do sandbox de uma aba
`sandmux enter [id]` e Ctrl+b `t` na TUI MUST abrir uma sessão nova com um shell dentro do sandbox da aba alvo viva, com:
- a mesma visão de arquivos, rede, Landlock e seccomp do comando da aba;
- as aberturas de arquivo pelo mesmo supervisor;
- o mesmo pid namespace.

Sem `id`, o alvo MUST ser a única sessão viva. Uma aba de dentro MUST recusar abrir outra, e uma aba alvo que não está viva MUST ser recusada com o motivo. Depois de um reinício do daemon, a aba de dentro MUST voltar parada.

#### Scenario: O que o agente vê
- **WHEN** a aba 1 roda um processo `sleep 4321` e tem um `.env` na pasta de trabalho, e o usuário roda `sandmux enter 1`
- **THEN** o shell está na mesma pasta, enxerga o `sleep 4321`, não lê o `.env` e não escreve em `/etc`

#### Scenario: Pela TUI
- **WHEN** na aba `shell:app` o usuário aperta Ctrl+b `t`
- **THEN** abre a aba `dentro:app`, com o shell dentro do sandbox de `shell:app`
