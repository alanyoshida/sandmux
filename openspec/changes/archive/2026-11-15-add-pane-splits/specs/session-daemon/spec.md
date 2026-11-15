## ADDED Requirements

### Requirement: Layouts de painéis guardados
O daemon MUST guardar os layouts de painéis (a árvore de divisões, com as sessões nas folhas) na lista de sessões e devolvê-los com a listagem. Um layout recebido MUST ter só sessões que existem, sem repetição, e MUST ser recusado com a mensagem do problema quando não tem. Uma sessão encerrada MUST sair do layout, e um layout com uma sessão só MUST deixar de existir. Os layouts MUST voltar na restauração.

#### Scenario: Depois de reiniciar o daemon
- **WHEN** a aba tem dois painéis e o daemon reinicia
- **THEN** a TUI aberta de novo mostra a aba com os mesmos dois painéis
