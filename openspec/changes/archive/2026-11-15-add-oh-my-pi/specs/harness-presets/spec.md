## ADDED Requirements

### Requirement: Preset do oh-my-pi
O preset `omp` MUST rodar o binário `~/.local/bin/omp` do host em somente leitura e MUST receber as chaves falsas dos provedores com segredo no cofre por variável de ambiente. Sem o binário, o `run` MUST terminar com a dica de instalar. A importação MUST copiar o `config.yml` e as pastas de extensões, agentes, regras e skills de `~/.omp/agent`, e MUST deixar de fora o `agent.db` (credenciais), os outros bancos, as sessões e os caches.

#### Scenario: Importação sem o banco de credenciais
- **WHEN** o usuário importa o oh-my-pi, e `~/.omp/agent/agent.db` existe
- **THEN** o HOME sintético tem `~/.omp/agent/config.yml` e as extensões, e não tem `agent.db`
