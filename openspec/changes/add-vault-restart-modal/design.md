# Design

## Context

O daemon já expõe, por sessão, `KeyNote.moved` (`crates/sandmux-proto/src/msg.rs`, protocolo 27): quantas chaves reais foram movidas ao cofre desde o último lançamento daquela sessão, zerado quando ela é reiniciada (`crates/sandmux/src/daemon/session.rs:1072`). A TUI já sincroniza esse campo em todo ciclo (`t.keys = s.keys` em `crates/sandmux/src/tui/mod.rs:706`) e já o usa para o texto passivo do `sandmux ls` e da seção Diagnóstico.

O padrão existente mais parecido é `Modal::Ended`: o laço de sincronização marca `t.ask_restart = true` quando uma aba termina, e `prompt_ended()` só abre o modal para a aba ativa, ou ao ativá-la depois. Esse padrão não serve aqui porque a proposta pede o aviso independente da aba ativa (ver proposal.md - Why).

O reinício forçado de hoje (`relaunch()`, `crates/sandmux/src/tui/mod.rs:1724`) presume a aba ativa (`self.tabs.get(self.active)`) e amarra a conexão nova em `self.tabs[self.active]` na hora. Isso não serve para reiniciar uma aba de fundo.

## Goals / Non-Goals

**Goals:**
- Detectar, no cliente da TUI, quando `keys.moved` de qualquer aba aumenta, sem depender de qual aba está ativa.
- Reiniciar, a partir do modal, uma aba que não é a ativa, sem recriar o mecanismo de reinício existente.

**Non-Goals:**
- Não muda `Modal::Ended`, `Modal::Restart` nem o texto do `sandmux ls`/Diagnóstico.
- Não acrescenta uma tecla em `[tui.keys]` para abrir o modal à mão: ele só abre sozinho, pela condição.
- Não muda o vigia de segredos nem o protocolo entre cliente e daemon.

## Decisions

**Estado novo por aba, só no cliente.** Acrescenta `vault_seen: u32` em `Tab`, ao lado de `keys: KeyNote`. Uma aba está pendente quando `t.keys.moved > t.vault_seen`. Como `keys.moved` zera no daemon quando a aba é reiniciada (por este cliente, por outro, ou por `sandmux restart --force`), a sincronização também zera `vault_seen` sempre que `s.keys.moved < t.keys.moved` (o reinício aconteceu), para que um aumento seguinte volte a ser detectado. Por ser só do cliente, cada TUI anexada decide por conta própria quando já mostrou um aumento — é exatamente por isso que duas TUIs podem discordar sobre o modal estar aberto (cenário da spec).

Alternativa descartada: guardar o "já visto" no daemon, por cliente. Mais correto para reconectar no meio, mas exige protocolo novo e nenhuma outra marca da TUI (nem `ask_restart`) faz isso hoje; o custo de reabrir o modal numa reconexão rara não paga a complexidade.

**Um modal, lista de abas.** `Modal::VaultRestart { ids: Vec<u32>, sel: usize }`, com `ids` as sessões pendentes no momento em que o modal abriu. Uma nova função `prompt_vault_restart()`, chamada no mesmo lugar que `prompt_ended()` (depois dela, então `Modal::Ended` continua tendo prioridade quando as duas condições coincidem), mas percorrendo `self.tabs` inteiro em vez de só `self.active`, e só abrindo quando `self.modal.is_none()`.

Alternativa descartada: um modal por aba, em fila (abordagem B da conversa com o usuário) — descartada porque uma chave só pode afetar várias abas do mesmo harness de uma vez (HOME compartilhado), e abrir vários modais em sequência para o mesmo evento é pior do que listar.

**Reiniciar sem presumir a aba ativa.** Em vez de `relaunch()`, o modal manda `Msg::Restart { id, vars, size: self.size, attach: false, force: true }` direto por `self.call(..)`, com `vars` calculado a partir de `t.opts` da aba escolhida (igual a `relaunch`, só que para um `id` qualquer, não `self.active`). Com `attach: false`, a conexão nova não precisa ser amarrada na hora: o laço de sincronização já anexa sozinho qualquer aba que passa de não-viva para viva com `conn.is_none()` (`crates/sandmux/src/tui/mod.rs:715-718`), o mesmo caminho que já trata uma aba reiniciada por outro terminal. Ao receber sucesso, a TUI tira o `id` da lista do modal (fechando o modal se a lista ficar vazia) e atualiza `vault_seen` daquela aba para o valor atual; ao receber erro, mostra na barra de status (`self.say`, como `relaunch` já faz) e deixa o `id` na lista para nova tentativa.

Alternativa descartada: generalizar `relaunch()` para aceitar um índice qualquer e amarrar a conexão fora da aba ativa. Funcionaria, mas or o código passaria a desenhar uma aba que não está na tela (trabalho sem efeito visível até o usuário trocar de aba) ou precisaria de um caminho de bind separado para abas de fundo; `attach: false` mais a reconciliação já existente evita as duas coisas.

**Teclas do modal, fixas.** ↑/↓ movem `sel`; `r` ou Enter reinicia a sessão selecionada; uma tecla reinicia todas (`a`, de "all"/"all"); Esc fecha sem reiniciar nada. Seguem o padrão de `Modal::Ended` (r/x/Esc) e `GroupsModal` (n/r/m): teclas fixas do modal, fora de `[tui.keys]`.

## Risks / Trade-offs

- Reiniciar uma aba de fundo pode falhar se um `pass-env` dela não estiver no ambiente do terminal da TUI → o erro aparece na barra de status e o `id` continua no modal para nova tentativa; é o mesmo risco que o Ctrl+b `r` já tem hoje na aba ativa.
- Com `attach: false`, a tela da aba reiniciada só é redesenhada no próximo ciclo de sincronização, não na hora → mesmo atraso que uma aba já tem hoje quando é reiniciada por outro terminal enquanto esta TUI está aberta; não bloqueia a interação com o modal nem com outras abas.
- Fechar o modal com Esc não deixa marca visível na tela depois → o texto do `sandmux ls` e da seção Diagnóstico continuam ali, como já decidido na proposta; não é um risco novo deste change.
