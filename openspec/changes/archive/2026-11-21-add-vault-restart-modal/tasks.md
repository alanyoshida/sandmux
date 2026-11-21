# Tasks

## 1. Estado do cliente e detecção

- [x] 1.1 Acrescentar `vault_seen: u32` em `Tab` (`crates/sandmux/src/tui/mod.rs`), inicializado a 0 onde `keys` já é inicializado (`tab_from` e o construtor de teste). Verificar com `cargo build -p sandmux`.
- [x] 1.2 No laço de sincronização de sessões, ao lado de `t.keys = s.keys`, zerar `t.vault_seen` quando `s.keys.moved < t.keys.moved` (a sessão foi reiniciada desde o último ciclo). Verificar com um teste de unidade em `crates/sandmux/src/tui/mod.rs` (`mod tests`) que simula dois ciclos de sincronização: `moved` sobe, cai a 0 por um reinício, sobe de novo, e `vault_seen` acompanha (sem reabrir o pendente por engano no meio).
- [x] 1.3 Escrever `fn pending_vault_restarts(&self) -> Vec<u32>`, retornando os ids das abas vivas com `t.keys.moved > t.vault_seen`, em ordem de id. Verificar com teste de unidade: duas abas, só uma com `moved > vault_seen`, a função retorna só essa.

## 2. Modal de aviso

- [x] 2.1 Acrescentar `Modal::VaultRestart { ids: Vec<u32>, sel: usize }` ao enum `Modal` (`crates/sandmux/src/tui/mod.rs`). Verificar que `cargo build -p sandmux` continua passando com o novo braço nos `match` existentes sobre `Modal` (adicionar os braços que faltarem).
- [x] 2.2 Escrever `fn prompt_vault_restart(&mut self)`, chamada no mesmo ponto do laço principal que já chama `prompt_ended()`, depois dela: se `self.modal.is_some()` ou `pending_vault_restarts()` está vazio, não faz nada; senão abre `Modal::VaultRestart { ids, sel: 0 }` e marca, para cada id da lista, `t.vault_seen = t.keys.moved` (não mostra de novo o mesmo aumento). Verificar com teste de unidade: com `Modal::Ended` já aberto, `prompt_vault_restart` não sobrescreve o modal, mesmo com chaves pendentes.
- [x] 2.3 Tratar as teclas do modal: ↑/↓ movem `sel` dentro dos limites da lista; Esc fecha o modal sem reiniciar nada; as teclas de reiniciar ficam para a tarefa 3 (precisam do pedido ao daemon). Verificar com teste de unidade do tratamento de tecla (sem pedir nada ao daemon): Esc fecha e a lista de pendências dos ids continua marcada como já vista.
- [x] 2.4 Desenhar o modal em `crates/sandmux/src/tui/render.rs`, centrado por cima da aba ativa (como os outros modais), com uma linha por sessão pendente (id, nome, harness) e a linha selecionada destacada. Verificar com teste de integração em `crates/sandmux/tests/tui.rs` (molde em `aba_encerrada_pergunta_se_reinicia`): uma chave real movida numa aba fora de foco abre o modal por cima da aba ativa, listando a sessão certa, sem trocar de aba.

## 3. Reiniciar a partir do modal

- [x] 3.1 Escrever `fn restart_from_vault_modal(&mut self, id: u32)`: monta `vars` a partir de `t.opts` da aba com aquele id (igual a `relaunch`, mas para um id qualquer) e manda `Msg::Restart { id, vars, size: self.size, attach: false, force: true }` por `self.call(..)`. Em caso de sucesso, tira o `id` da lista do modal (fechando o modal se ficar vazia) e zera `t.vault_seen` daquela aba; em erro, mostra na barra de status com `self.say(..)` e deixa o `id` na lista. Verificar com teste de unidade usando um daemon falso: sucesso tira o id da lista, erro mantém.
- [x] 3.2 Ligar `r`/Enter (reinicia a sessão selecionada) e uma tecla de "reiniciar todas" ao `restart_from_vault_modal`, chamando-o para cada id quando for "todas". Verificar com teste de integração: duas abas vivas do mesmo harness com chave movida, "reiniciar todas" deixa as duas de novo vivas e o modal fecha.
- [x] 3.3 Teste de integração: fechar o modal com Esc, mover outra chave numa aba ainda não vista nessa leva, e confirmar que o modal abre de novo só com a aba nova (`crates/sandmux/tests/tui.rs`).
- [x] 3.4 Teste de integração: reiniciar uma sessão pelo modal enquanto outra aba está ativa, e confirmar que a aba reiniciada aparece viva (conteúdo novo) depois do próximo ciclo de sincronização, sem a TUI trocar de aba sozinha.

## 4. Verificação manual

- [x] 4.1 Com uma sessão real do opencode e a chave do OpenRouter configurada dentro dela (gatilho do caso original), confirmar que o modal aparece mesmo com outra aba em foco, que "reiniciar" devolve a sessão funcionando com a chave real trocada no tráfego, e que a notificação de desktop (change 48) e o texto do `sandmux ls`/Diagnóstico continuam como antes. Feito em 2026-11-21, num `main` compilado numa worktree isolada (vault e daemon próprios, nada tocou nas sessões reais): o modal apareceu por cima da aba ativa listando a sessão certa; o `sandmux net` confirmou a troca da chave real (`chave: opencode:.../auth.json:openrouter.key` num `POST` real ao `openrouter.ai`), e o erro mudou de "Missing Authentication header" para um 401 "API key expired" (a chave de teste do `.env` está expirada, sem relação com o sandmux). A verificação achou um 3º bug (aba de fundo reiniciada pelo modal podia ficar sem reconectar, por uma corrida com a sincronização de 1s), corrigido e mergeado no PR alanyoshida/sandmux#93. A notificação de desktop (change 48) não foi exercida: exige perda de foco do terminal no nível do sistema, que este método (tmux) não reproduz.
- [x] 4.2 Rodar `./scripts/check.sh` (ou o equivalente com o vigia de memória, conforme `heavy_tests_freeze`) e confirmar a suíte inteira verde.

## Workflow follow-up

- Atualizar `openspec/roadmap.md` (seção 4, etapa 3, e seção 7 se houver pedido original ligado) com a situação do change.
- Arquivar o change depois da revisão do usuário.
