## Why

Todo texto do sandmux está em português. O usuário pediu (2026-11-14) poder trocar para inglês, com uma opção na página de configurações. Decisões dele:
- muda tudo: a TUI, a linha de comando, o daemon, as mensagens de validação da configuração e, no change 37, a documentação;
- sem nada escolhido, o sandmux segue o idioma do sistema.

## What Changes

- **Dois idiomas:** português (`pt`) e inglês (`en`). O texto de cada mensagem fica no código, nos dois idiomas, lado a lado (`tr!` e `trf!`), sem arquivo de catálogo à parte.
- **Escolha:** a chave `language` em `[global]` (`"pt"` ou `"en"`; só no global, como `net-log-days`).
  - Sem a chave, vale o sistema: `LC_ALL`, depois `LC_MESSAGES`, depois `LANG`. `pt*` é português; qualquer outro, inclusive `C` ou vazio, é inglês.
  - `SANDMUX_LANG` (`pt` ou `en`) passa por cima de tudo, para scripts e testes.
- **Página de configurações:** uma seção nova, "Geral" ("General"), com "Idioma" (`‹ auto ›`, `‹ pt ›`, `‹ en ›`). A TUI troca de idioma na hora. O daemon relê o idioma a cada pedido.
- **Traduzido:** tudo que o usuário lê:
  - a TUI: barra, painel, modais, menu, ajuda e página de configurações;
  - a saída e os erros da linha de comando;
  - os avisos do daemon e do supervisor;
  - as mensagens de validação do `config.toml`.
- **Fica igual:** os nomes das chaves, das seções, dos valores do arquivo (`aberta`, `inferencia`, `blocos`…), dos subcomandos e das opções da linha de comando. Mudá-los quebraria os arquivos e scripts existentes.
- **Fora deste change:** a documentação, no change 37 (`add-english-docs`).

## Capabilities

### New Capabilities
- `interface-language`: o idioma da interface, a escolha e o que muda com ele.

### Modified Capabilities
(nenhuma: as mensagens existentes ganham a versão em inglês, sem mudar o comportamento)

## Impact

- **`sandmux-spec`:** módulo `lang` (idioma atual, leitura do sistema, `tr!` e `trf!`); todos os crates com texto ao usuário passam a depender dele.
- **`sandmux-policy`:** a chave `language`, e as mensagens de validação nos dois idiomas.
- **`sandmux`:** os binários definem o idioma ao começar; a TUI e a página de configurações; todas as mensagens.
- **Testes:**
  - o ambiente de teste fixa `LANG=pt_BR.UTF-8`, e os testes de antes continuam em português;
  - testes novos em inglês.
- **Documentação:** `docs/config.md` e `docs/tui.md` citam o idioma.
