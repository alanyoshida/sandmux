## 1. Base

- [x] 1.1 `sandmux_spec::lang`: idioma atual, leitura do sistema e de `SANDMUX_LANG`, `tr!` e `trf!`, com testes
- [x] 1.2 Chave `language` em `[global]` (`GLOBAL_ONLY_KEYS`), validada; `lang::init` nos binários; o daemon relê a cada pedido; `SANDMUX_LANG` para o supervisor e a aba de chat
- [x] 1.3 Página de configurações: seção "Geral" com "Idioma", textos da página nos dois idiomas, troca na hora

## 2. Tradução

- [x] 2.1 TUI: barra, painel, modais, menu, ajuda, telas de rede, arquivos, pastas e MCP
- [x] 2.2 Linha de comando: saída e erros de todos os subcomandos
- [x] 2.3 Daemon, supervisor, launcher, rede, importação, cofre e mensagens de validação da configuração

## 3. Verificação

- [x] 3.1 Ambiente de teste com `LANG=pt_BR.UTF-8`; testes em inglês (TUI, troca pela página, erro da linha de comando, precedência)
- [x] 3.2 Busca de texto em português fora de `tr!` nos crates
- [x] 3.3 `docs/config.md` e `docs/tui.md` citam o idioma
