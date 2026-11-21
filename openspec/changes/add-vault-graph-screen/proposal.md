## Why

O usuário pediu (2026-11-17) uma página que mostre o cofre como um grafo: o domínio no centro, com arestas para os segredos cadastrados para ele, pelo nome e nunca pelo valor, com zoom e movimento em todas as direções. Hoje a seção Cofre mostra uma linha por segredo, com os domínios em texto. Nessa lista não dá para ver, a partir de um domínio, quais segredos podem ir para ele, nem quais segredos valem em mais de um domínio ou em nenhum.

## What Changes

- **Tela do grafo:** na seção Cofre da página de configurações, a tecla `g` abre uma tela cheia com três partes:
  - à esquerda, a lista dos domínios donos, com o número de segredos de cada um, e no fim os grupos "Sem domínio" e "Bancos";
  - no meio, o grafo do domínio escolhido: o domínio numa caixa no centro, os segredos em volta e uma aresta para cada um;
  - à direita, os detalhes do segredo em foco: nome, tipo, valor falso, ferramentas e todos os domínios donos.
- **Sem valores:** o valor de um segredo não aparece em nenhuma parte da tela.
- **Zoom e movimento:** `+` e `-` mudam o zoom, `h` `j` `k` `l` movem o grafo e `0` volta ao começo. A roda do mouse muda o zoom e arrastar move. O texto não muda de tamanho: o zoom muda o espaço entre os nós e quanto cada nó mostra (só a marca, a marca e o nome, ou um cartão com o tipo e as ferramentas).
- **Edição na própria tela:** as teclas `a`, `s`, `d`, `v` e `x` da seção Cofre valem na tela do grafo, com os mesmos formulários. Um segredo novo já vem com o domínio escolhido preenchido. Depois de cada mudança o grafo é redesenhado.
- **Bancos:** as conexões de banco do cofre aparecem no grupo "Bancos", só para leitura, com o tipo, o servidor, a porta, o banco e o usuário, nunca a senha. Elas continuam sendo gerenciadas por `sandmux db`.
- **Busca:** `/` filtra a lista de domínios pelo nome do domínio ou de um segredo.
- **Terminal estreito:** com menos de 100 colunas os detalhes abrem com Enter, sobre o grafo; com menos de 70, a tela mostra só a lista.
- **Arestas sem Braille:** as arestas usam os pontos do Braille. A chave nova `graph-edges` de `[tui]` troca para meio-bloco, para fontes sem Braille, e aparece na seção Aparência.
- **Fora deste change:** a visão do cofre inteiro numa só tela (constelação, roda ou bolhas, exploradas em 2026-11-17), a edição das conexões de banco pela TUI e a edição ligando nós com o mouse.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `secret-vault`: a tela do grafo do cofre, o que ela mostra (segredos e conexões de banco) e a edição do cofre por ela.
- `terminal-ui`: a navegação do grafo (teclas, mouse, zoom e movimento), o grafo em terminal estreito e a chave `graph-edges` de `[tui]`.

## Impact

- **`sandmux`:**
  - `tui/vaultgraph.rs` (novo): o modelo do grafo, as posições e a janela de zoom e movimento;
  - `tui/settings.rs`: o estado da tela dentro da tela do cofre, as teclas e o item de Aparência;
  - `tui/render.rs`: o desenho da tela, com o `Canvas` do ratatui para as arestas;
  - `tui/mod.rs`: o mouse (roda, arrastar e clique) na tela do grafo.
- **`sandmux-policy`:** a chave `graph-edges` de `[tui]`, na leitura e no editor de configuração.
- **Protocolo:** sem mudança. A tela usa `VaultList`, `DbList` e as operações do cofre que já existem.
- **Dependências:** nenhuma nova. O `Canvas` já vem com o ratatui 0.30.
- **Testes e documentação:** testes do modelo e das posições, testes da TUI; `docs/vault.md`, `docs/tui.md`, `docs/config.md` e as versões em inglês.
