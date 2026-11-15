## ADDED Requirements

### Requirement: TUI por grupo
A TUI MUST mostrar só as abas do seu grupo atual e o nome do grupo na barra. Uma aba criada na TUI MUST ir para o grupo atual. A TUI MUST abrir no grupo de `--group`; sem ele, no `principal`, ou, com o `principal` vazio, no primeiro grupo com abas. Cada TUI MUST ter o seu grupo, sem mudar o de outra TUI aberta.

#### Scenario: Duas TUIs
- **WHEN** uma TUI mostra o grupo `principal` e outra, aberta com `--group estudos`, mostra o grupo `estudos`
- **THEN** cada uma mostra só as abas do seu grupo, e uma aba criada em uma não aparece na outra

### Requirement: Troca de grupo
Ctrl+b `(` e Ctrl+b `)` MUST ir ao grupo anterior e ao próximo, em ordem de nome. Ctrl+b `G` MUST abrir a lista de grupos, com o número de abas de cada um:

- Enter troca para o grupo escolhido;
- `n` pede um nome e troca para um grupo novo, vazio até a primeira aba;
- `r` renomeia o grupo escolhido;
- `m` leva a aba ativa para o grupo escolhido.

Ao trocar de grupo, a TUI MUST desanexar as abas do grupo que sai da tela, sem encerrá-las.

#### Scenario: Próximo grupo
- **WHEN** existem os grupos `estudos` e `principal`, a TUI mostra `estudos`, e o usuário aperta Ctrl+b `)`
- **THEN** a TUI passa a mostrar as abas do `principal`, e as abas de `estudos` continuam vivas

#### Scenario: Mover a aba ativa
- **WHEN** o usuário abre a lista de grupos, escolhe `estudos` e aperta `m`
- **THEN** a aba ativa sai da tela e passa a ser do grupo `estudos`
