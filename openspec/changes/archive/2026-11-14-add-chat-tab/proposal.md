## Why

O usuário quer conversar com os modelos que já cadastrou sem abrir um harness inteiro: uma pergunta rápida, uma dúvida sobre um erro, uma explicação. Hoje a única forma de falar com um modelo pelo sandmux é rodar o opencode ou o hermes numa pasta de trabalho. A etapa 9 começa pela aba de chat; o modal de comando shell (change 22) vem depois e reaproveita a mesma ligação com os modelos.

## What Changes

- **Modal de chat na TUI:** prefixo seguido de `i` abre um modal com a mensagem e o modelo. Enter envia; a resposta abre numa aba de chat nova. O modelo vem de `[assistant] chat-model` e pode ser trocado no modal, escrevendo outro ou passando pelos de `[assistant] models` com as setas.
- **Aba de chat:** uma sessão do daemon como as outras, que roda o programa `sandmux-chat` dentro do sandbox. Ele desenha a conversa, uma barra de texto e um botão "Enviar" (Enter ou clique). As respostas chegam aos poucos (streaming). `/modelo <nome>` troca o modelo da conversa.
- **Confinamento da aba de chat:**
  - rede sempre no modo só inferência, mais o host dos provedores próprios da configuração;
  - a chave real nunca entra: o programa recebe o valor falso do cofre, e o gateway troca no tráfego, como nos harnesses;
  - sem Docker privado, agente SSH, MCP de banco nem repositório;
  - a pasta de trabalho é uma pasta privada da aba, em `$XDG_STATE_HOME/sandmux/chat/<id>`.
- **Conversa em disco (decisão do usuário):** a conversa fica em `conversa.json` na pasta da aba e volta depois de um reinício do daemon ou de um reboot. Fechar a aba apaga a pasta.
- **Cofre:** com o cofre trancado, o modal abre o desbloqueio antes de enviar. Depois de um reboot, a aba de chat fica "aguardando cofre" enquanto o cofre existe e está trancado.
- **Provedores:** o modelo se escreve `<provedor>/<modelo>`. Os provedores conhecidos são os da tabela de inferência (`openrouter`, `anthropic`, `openai`, `gemini`, `deepseek`, `mistral`, `groq` e `xai`), todos pela API compatível com a da OpenAI. `[assistant.provider.<nome>]` com `url` cadastra outro endpoint compatível, por exemplo um servidor local; a chave dele é o segredo do cofre dono do host da URL, se houver.
- **Linha de comando:** `sandmux chat [--model M] [TEXTO]`. Com terminal, cria a aba de chat no daemon e anexa; sem terminal, envia o texto, imprime a resposta e termina, sem guardar nada.
- **Configuração:** a seção nova `[assistant]` (`chat-model`, `models` e `[assistant.provider.<nome>]`) e a ação `chat` em `[tui.keys]`.
- **Protocolo 20:** o daemon passa a aceitar sessões de chat.

## Capabilities

### New Capabilities
- `assistant-chat`: o modal de chat, a aba de chat e o programa dela, os modelos e provedores, a conversa guardada, o confinamento próprio da aba e o `sandmux chat`.

### Modified Capabilities
- `config-profiles`: a seção `[assistant]` passa a ser aceita.
- `key-injection`: as chaves falsas dos provedores entram também nas sessões de chat.
- `session-daemon`: a sessão de chat aguarda o cofre depois de um reboot, mesmo sem `pass-env`.

## Impact

- **`sandmux-policy`:**
  - `config.rs`: a seção `[assistant]`, os provedores próprios e a ação `chat`;
  - `presets.rs`: o preset interno `chat`, a tabela de provedores e as chaves falsas;
  - `compose.rs`: o binário `sandmux-chat` montado em `/run/sandmux/bin`.
- **`sandmux`:**
  - binário novo `sandmux-chat` (ratatui sobre o PTY, cliente HTTP com streaming);
  - `main.rs`: o comando `chat`;
  - `tui/`: o modal de chat e a ação `chat`;
  - `daemon/`: a sessão de chat sem Docker, sem SSH e sem banco, a pasta apagada ao fechar e a espera pelo cofre na restauração;
  - `launch.rs`: o modo de rede e as regras da aba de chat.
- **`sandmux-proto`:** protocolo 20.
- **Documentação:** `docs/chat.md` novo, `docs/config.md`, `docs/sessions.md` e o README.
- **Dependências:** nenhuma nova; o cliente HTTP usa `hyper`, `hyper-util` e `tokio-rustls`, que o workspace já tem.
