## Why

O primeiro item de "Depois da v1" escolhido pelo usuário (2026-11-15) é entrar no sandbox de uma aba: abrir um shell dentro do sandbox de uma aba em execução, para ver exatamente o que o agente vê.

## What Changes

- **Atalhos:** Ctrl+b `t` na TUI e `sandmux enter [id]` na linha de comando abrem uma aba nova, `dentro:<pasta>`, com um shell dentro do sandbox da aba alvo.
- **Mesmo confinamento do agente:** o shell tem a mesma visão de arquivos, a mesma rede, o mesmo Landlock e o mesmo seccomp, e as aberturas de arquivo passam pelo mesmo supervisor (segredos negados, registro de acessos). Ele também enxerga os processos do agente, porque está no mesmo pid namespace.
- **Ciclo de vida:**
  - a aba de dentro é uma sessão comum e termina quando o shell sai;
  - se o sandbox alvo termina, o shell termina junto;
  - depois de um reinício do daemon, a aba de dentro volta parada, com o motivo, porque o sandbox alvo é outro.
- **Alvo:**
  - sem `id`, a única sessão viva;
  - uma aba de dentro não abre outra.
  - uma aba lançada por uma versão anterior não aceita entrar; ela precisa ser reiniciada.

- **Teclas:** a tecla `t` nova colidia com quem já tinha trocado outra ação para `t`, e o arquivo ficava inválido. Agora uma tecla escolhida em `[tui.keys]` vence a tecla padrão de outra ação, que fica sem tecla. Duas teclas escolhidas iguais continuam sendo erro.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `terminal-sessions`: o shell dentro do sandbox de uma aba.
- `terminal-ui`: a tecla escolhida vence a padrão de outra ação.

## Impact

- **`sandmux-launcher`:**
  - `enter.rs` novo: o auxiliar que entra nos namespaces e abre o shell, e o lado de quem pede;
  - `launch.rs`: o socket `<controle>.enter` e as aberturas do shell no supervisor de arquivos;
  - `files.rs`: o auxiliar de nomes compartilhado.
- **`sandmux`:**
  - `sandmux-run --enter`;
  - o daemon: `ENTER` (`dentro`), a escolha da aba alvo, o lançamento e a restauração;
  - a TUI: a ação `enter`, tecla `t`;
  - `sandmux enter`.
- **Testes e documentação:** testes de integração; `docs/sessions.md`, `docs/tui.md` e as versões em inglês.
