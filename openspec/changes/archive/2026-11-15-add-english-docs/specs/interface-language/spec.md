## ADDED Requirements

### Requirement: Documentação em inglês
Cada arquivo de `docs/` MUST ter a versão em inglês em `docs/en/` com o mesmo nome, e o README MUST ter `README.en.md`, com links entre as duas versões. Os exemplos de configuração da versão em inglês MUST ser válidos.

#### Scenario: Par de cada doc
- **WHEN** existe `docs/network.md`
- **THEN** existe `docs/en/network.md`, e o README em português aponta para o `README.en.md`
