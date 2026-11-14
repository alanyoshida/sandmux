## 1. Configuração e provedores

- [x] 1.1 Em `sandmux-policy`, a seção `[assistant]` (`chat-model`, `models`, `[assistant.provider.<nome>]` com `url`), a validação com linha e chave, e a ação `chat` (tecla `i`) em `TUI_ACTIONS`, com testes
- [x] 1.2 Módulo `assistant` com a tabela de provedores conhecidos, a divisão `provedor/modelo`, a URL do pedido e o host de cada provedor, com testes

## 2. Programa sandmux-chat

- [x] 2.1 Conversa em `conversa.json` (leitura, gravação atômica, reenvio da pergunta sem resposta) e cliente HTTP com streaming SSE sobre `hyper` e `tokio-rustls`, com raízes de `SSL_CERT_FILE`, com testes contra um servidor local
- [x] 2.2 Interface em ratatui: conversa com rolagem, linha do modelo, barra de texto, botão "Enviar" com clique, Ctrl+j, `/modelo`, erro do provedor na conversa
- [x] 2.3 Modo `--once`: resposta sem streaming na saída padrão, código de saída do erro

## 3. Daemon e lançamento

- [x] 3.1 Harness interno `chat`: binário montado em `/run/sandmux/bin`, `SANDMUX_CHAT_PROVIDERS` e chaves falsas, rede só inferência com os hosts dos provedores próprios, sem Docker, SSH, banco, repositório, pastas extras e `pass-env`
- [x] 3.2 Pasta da aba apagada ao fechar, "aguardando cofre" na restauração, nome `chat:<modelo>`, `Msg::ChatCheck` e `Msg::Ask` com o lançamento em pipe; protocolo 20

## 4. Cliente

- [x] 4.1 Modal de chat na TUI: campos, setas pelos modelos, desbloqueio antes de criar, recusa sem chave, criação da pasta e da aba
- [x] 4.2 Comando `sandmux chat [--model M] [TEXTO]`, com e sem terminal

## 5. Verificação e documentação

- [x] 5.1 Testes de integração com um provedor falso por HTTPS: conversa com troca da chave, streaming, persistência depois de `daemon restart`, pasta apagada ao fechar, rede bloqueada fora da inferência, `sandmux chat` sem terminal
- [x] 5.2 `docs/chat.md`, `docs/config.md`, `docs/sessions.md`, README e teste de documentação
- [x] 5.3 Verificação à mão com o modelo real do OpenRouter e a chave do `.env` no cofre de teste: aba de chat com duas perguntas, reinício do daemon com a conversa de volta, chave real em nenhum arquivo do sandbox
