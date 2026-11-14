# Tasks

## 1. Cofre

- [x] 1.1 Criar o crate `sandmux-vault` (formato do arquivo, Argon2id, XChaCha20-Poly1305 com o cabeçalho como dado associado, conteúdo com `zeroize`, `Store` e `FileStore` com gravação atômica e modo 600); verificar com testes de unidade de ida e volta, senha errada, cabeçalho adulterado, nada em claro e troca de senha
- [x] 1.2 Acrescentar ao protocolo os pedidos do cofre, o estado `AguardandoCofre` e o `PROTOCOL` 10; verificar com teste de unidade de serialização

## 2. Daemon e linha de comando

- [x] 2.1 Guardar o estado do cofre no daemon e atender os pedidos; criar `sandmux vault init|unlock|lock|status|add|list|remove|passwd` com a leitura sem eco e pela entrada padrão; marcar o daemon como não despejável; verificar com testes de CLI pela entrada padrão (criação, desbloqueio, senha errada, cofre trancado, listagem sem valores, troca de senha, nada em claro no arquivo)
- [x] 2.2 Guardar os valores de `--pass-env` no cofre, restaurar as sessões como "aguardando cofre", relançá-las no desbloqueio e limpar os valores no `kill`; avisar quando não dá para guardar; verificar com teste de PTY de uma sessão com `--pass-env` que volta depois de `sandmux daemon restart` e do desbloqueio, com o valor
- [x] 2.3 Criar `sandmux vault import` a partir dos manifestos da importação; verificar com teste de CLI com um manifesto de teste

## 3. TUI

- [x] 3.1 Criar o modal de desbloqueio na abertura da TUI e o estado "aguardando cofre" na barra; verificar com teste de PTY de senha errada e certa

## 4. Documentação e verificação

- [x] 4.1 Escrever `docs/vault.md` e atualizar `docs/sessions.md`, `docs/tui.md` e o README; verificar com o teste de documentação
- [x] 4.2 Rodar `./scripts/check.sh` sem falhas
- [x] 4.3 Verificar à mão uma sessão real do opencode com a chave do OpenRouter por `--pass-env`, guardada no cofre, que volta sozinha depois de `sandmux daemon restart` e do desbloqueio, e responde; e a chave em nenhum arquivo em claro
