## MODIFIED Requirements

### Requirement: Itens importados do hermes
Para o hermes, a importação MUST copiar de `~/.hermes` o `config.yaml`, o `SOUL.md` e as pastas `skills`, `memories`, `plugins` e `hooks`, e MUST copiar `.env` e `auth.json` com os segredos trocados. O pareamento de canais de mensagem e as tarefas agendadas MUST ficar de fora: eles são criados dentro do sandbox.

#### Scenario: Memórias
- **WHEN** o usuário tem memórias em `~/.hermes/memories`
- **THEN** o hermes confinado as encontra depois da importação

#### Scenario: Canais e agendamentos
- **WHEN** o estado real do hermes tem pareamento de canais de mensagem e tarefas agendadas
- **THEN** nada disso é copiado, e o relatório lista `pairing` e `cron` como ignorados
