## ADDED Requirements

### Requirement: Foco do terminal para as notificações
A TUI MUST ligar o aviso de foco do terminal enquanto está aberta e MUST informar ao daemon quando ganha ou perde o foco e qual aba está mostrando, também ao trocar de aba. Os eventos de foco MUST NOT chegar à aba como texto. Eles MUST ser repassados à aba só quando o harness dela pediu o aviso de foco. A página de configurações MUST ter, na seção Geral, o item que liga e desliga as notificações (`[global] notify`).

#### Scenario: Harness sem aviso de foco
- **WHEN** a TUI perde o foco e o harness da aba ativa não pediu o aviso de foco
- **THEN** a aba não recebe nada, e o daemon fica sabendo que a TUI está sem foco

#### Scenario: Harness com aviso de foco
- **WHEN** o harness da aba ativa pediu o aviso de foco e a TUI ganha o foco
- **THEN** a aba recebe o aviso de foco, como receberia num terminal comum
