## Why

O usuário pediu (2026-11-14) uma opção para gerenciar o cofre na página de configurações da TUI. Hoje o cofre só se gerencia pela linha de comando (`sandmux vault …`), e o modal de desbloqueio da TUI só abre o cofre.

## What Changes

- **Seção nova "Cofre" ("Vault")** na página de configurações. Ela mostra:
  - o estado: sem cofre, trancado ou aberto, com o número de segredos;
  - a lista de segredos: nome, tipo (token ou chave SSH) e domínios, nunca o valor.
- **Ações por tecla:**
  - criar o cofre;
  - desbloquear e trancar;
  - trocar a senha;
  - acrescentar um segredo: nome, valor sem eco, domínios;
  - acrescentar uma chave SSH: arquivo da chave e servidores;
  - editar os domínios de um segredo;
  - apagar um segredo, com confirmação.
- **Campos de senha e de valor:** mostram `•` por caractere e são apagados da memória (zeroize) depois de enviados ao daemon.
- **Protocolo 24:** `VaultSetDomains { name, domains }` troca os domínios de um segredo sem pedir o valor de novo.
- **Idioma:** os textos nascem nos dois idiomas, pelo mecanismo do change 36.
- **Fora deste change:** as conexões de banco (`sandmux db`), que continuam na linha de comando.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `secret-vault`: a gestão do cofre pela TUI e a troca dos domínios de um segredo.

## Impact

- **`sandmux-proto`:** `VaultSetDomains`, protocolo 24.
- **`sandmux`:**
  - `daemon/vault.rs`: a troca de domínios;
  - `tui/settings.rs` e `tui/render.rs`: a seção, os formulários e as teclas.
- **Testes e documentação:** testes da TUI; `docs/vault.md`, `docs/tui.md` e as versões em inglês.
