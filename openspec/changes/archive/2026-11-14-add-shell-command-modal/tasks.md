## 1. Base

- [x] 1.1 `command-model` em `[assistant]` e a ação `command` (tecla `g`) em `TUI_ACTIONS`; `assistant::command_prompt` e `assistant::clean_command`, com testes
- [x] 1.2 Texto de sistema na conversa (`Conversa.sistema`, mandado como `system` pelo `sandmux-chat`) e `Msg::Ask.system`; protocolo 21

## 2. TUI

- [x] 2.1 Modal de comando: campos, setas pelos modelos, conferência do cofre e da chave, desbloqueio antes de perguntar, pergunta numa thread com `Ev::Answer`, espera e erro no modal
- [x] 2.2 Campo do comando editável, Enter e botão "Inserir", escrita na aba sem `\r`, colagem entre marcadores com várias linhas e recusa sem o modo de colagem

## 3. Verificação e documentação

- [x] 3.1 Testes de integração com o provedor falso: o que o provedor recebe (pergunta, pasta, shell, nada da tela), o comando escrito no shell sem executar, várias linhas, erro do provedor
- [x] 3.2 `docs/chat.md`, `docs/tui.md`, `docs/config.md`, `docs/sessions.md`, README e teste de documentação
- [x] 3.3 Verificação à mão com o modelo real do OpenRouter e a chave do `.env` num cofre de teste: um comando gerado, escrito no shell sem executar, e a chave real em nenhum arquivo
