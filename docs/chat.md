# Chat com os modelos

A aba de chat conversa com um modelo cuja chave está no [cofre](vault.md), sem abrir um harness. Ela abre pelo modal da TUI (Ctrl+b `i`) ou por `sandmux chat`. O [modal de comando](#modal-de-comando) (Ctrl+b `g`) usa os mesmos modelos para escrever um comando na aba.

## Abrir uma conversa

Na TUI, Ctrl+b `i` abre o modal de chat:

- **mensagem:** a primeira pergunta;
- **modelo:** começa com `chat-model` de `[assistant]`. As setas ↑ ↓ passam pelos modelos de `models`, e qualquer outro nome pode ser escrito.

Tab troca de campo, Enter envia e Esc cancela. A resposta abre numa aba de chat nova, com o nome `chat:<modelo>`.

Pela linha de comando:

```sh
sandmux chat "o que é um inode?"
sandmux chat --model openrouter/anthropic/claude-sonnet-5
```

- **Com terminal:** cria a aba de chat no daemon e anexa, como `sandmux run`. Ctrl+b d desanexa.
- **Sem terminal (script, pipe):** precisa do texto. Imprime só a resposta na saída padrão e termina, sem guardar nada. Um erro do provedor termina com código 1.

## Dentro da aba

A aba mostra o modelo no topo, a conversa no meio e, embaixo, a barra de texto e o botão `[ Enviar ]`.

| Tecla | O que faz |
|---|---|
| Enter ou clique em `[ Enviar ]` | Envia o texto da barra |
| Ctrl+j ou Alt+Enter | Quebra a linha |
| Ctrl+u | Apaga a barra |
| Ctrl+c | Cancela a resposta em curso |
| PgUp / PgDn e a roda do mouse | Rolam a conversa |

- A resposta aparece aos poucos, conforme chega (streaming).
- `/modelo <provedor>/<modelo>` troca o modelo das mensagens seguintes. `/modelo` sozinho mostra o atual e os provedores.
- Um erro do provedor aparece na conversa, com o status e a mensagem dele, e a conversa continua.
- O texto aparece como veio, sem renderizar Markdown. A aba de chat não tem ferramentas: o modelo não lê arquivos nem roda comandos.

## Modelos e provedores

O modelo se escreve `<provedor>/<modelo>`. A primeira barra separa o provedor; o resto vai ao provedor como nome do modelo. Todos os provedores são usados pela API compatível com a da OpenAI (`<url>/chat/completions`).

| Provedor | URL base | Chave no cofre (domínio) |
|---|---|---|
| `openrouter` | `https://openrouter.ai/api/v1` | `openrouter.ai` |
| `anthropic` | `https://api.anthropic.com/v1` | `api.anthropic.com` |
| `openai` | `https://api.openai.com/v1` | `api.openai.com` |
| `gemini` | `https://generativelanguage.googleapis.com/v1beta/openai` | `generativelanguage.googleapis.com` |
| `deepseek` | `https://api.deepseek.com/v1` | `api.deepseek.com` |
| `mistral` | `https://api.mistral.ai/v1` | `api.mistral.ai` |
| `groq` | `https://api.groq.com/openai/v1` | `api.groq.com` |
| `xai` | `https://api.x.ai/v1` | `api.x.ai` |

A chave de um provedor conhecido precisa estar no cofre, ligada ao domínio dele:

```sh
sandmux vault add openrouter --domain openrouter.ai
```

Sem ela, o modal e o `sandmux chat` recusam e mostram esse comando. Com o cofre trancado, o modal abre o desbloqueio antes, e o `sandmux chat` num terminal pede a senha.

### Provedor próprio

`[assistant.provider.<nome>]` cadastra outro endpoint compatível, como um servidor local:

```toml
[assistant]
chat-model = "openrouter/deepseek/deepseek-v4.1-flash-1017"
command-model = "openrouter/deepseek/deepseek-v4.1-flash-1017"
models = ["openrouter/anthropic/claude-sonnet-5", "local/llama3"]

[assistant.provider.local]
url = "https://host.sandmux.internal:8443/v1"
```

- **Rede:** o host da URL é liberado para as abas de chat.
- **Chave:** se o cofre tem um segredo cujo domínio é o host da URL, a chave vai trocada no tráfego. Sem segredo, o pedido vai sem `Authorization`.
- **HTTP:** a troca só acontece em HTTPS. Um servidor local por `http://` funciona, mas sem chave.
- **Nomes:** o nome de um provedor próprio não pode repetir o de um conhecido.

## Confinamento

A aba de chat é uma sessão do daemon que roda o programa `sandmux-chat` dentro do sandbox, como os harnesses. O binário fica ao lado do `sandmux-run` e entra em somente leitura em `/run/sandmux/bin/sandmux-chat`.

- **Rede:** sempre no modo só inferência, mais o host de cada provedor próprio, qualquer que seja o `network` das camadas. As regras de bloqueio da configuração continuam valendo, e cada pedido vai para o [registro de rede](network.md).
- **Chave:** o programa recebe só o valor falso do cofre (`OPENROUTER_API_KEY` e as outras variáveis dos provedores, ou `SANDMUX_CHAT_KEY_<NOME>` num provedor próprio). O gateway troca pelo real no tráfego, como nos harnesses. Os provedores chegam ao programa em `SANDMUX_CHAT_PROVIDERS`.
- **Sem extras:** sem Docker privado, agente SSH, MCP de banco, repositório, pastas extras nem `pass-env`.
- **Pasta:** a pasta de trabalho é uma pasta privada da aba, em `$XDG_STATE_HOME/sandmux/chat/<id>` (modo 700).

## A conversa guardada

A conversa fica em `conversa.json`, na pasta da aba, com o modelo e as mensagens.

- **Quando é gravada:** a cada mensagem enviada e a cada resposta completa, num arquivo temporário renomeado.
- **Resposta interrompida:** não é gravada. Na volta, a última pergunta não tem resposta, e o programa a envia de novo.
- **Reinício do daemon e reboot:** a aba volta com a conversa. Com o cofre criado e trancado, ela fica "aguardando cofre" e sobe no desbloqueio, como o [gateway do hermes](hermes.md).
- **Ao fechar a aba** (Ctrl+b `x` ou `sandmux kill`), a pasta e a conversa são apagadas.
- **Lista de sessões:** a conversa não vai para a lista do daemon. A primeira mensagem também não: ela entra direto no `conversa.json`.

Esta é uma exceção à regra do histórico de tela, que nunca vai para o disco, e vale só para a aba de chat. Foi decidida pelo usuário em 2026-11-14.

## Modal de comando

Ctrl+b `g` abre, sobre a aba ativa, o modal de comando: uma pergunta como "comprimir a pasta logs sem os .tmp" vira um comando escrito na linha da aba.

1. **Pergunta e modelo:** o modelo começa com `command-model` de `[assistant]` ou, sem ele, com `chat-model`. Tab e as setas funcionam como no modal de chat.
2. **Enter pergunta.** Vão ao modelo só a pergunta, a pasta de trabalho da aba e o nome do shell (o `SHELL` do usuário quando está nas pastas de sistema; senão, `bash`), com um texto de sistema que pede um único comando, sem explicação. Nada da tela da aba é enviado. A TUI continua respondendo enquanto o modal mostra "esperando o modelo".
3. **O comando sugerido** aparece num campo editável, sem as cercas de código e sem um `$ ` no começo. Tab volta à pergunta.
4. **Enter, ou o clique em `[ Inserir ]`,** escreve o comando na entrada da aba, sem Enter: ele só roda quando o usuário apertar Enter na aba.

Um comando de várias linhas vai como colagem (entre `ESC[200~` e `ESC[201~`) quando a aba ligou o modo de colagem, como o bash e o zsh fazem. Sem esse modo, o modal recusa, porque a quebra de linha executaria o comando.

A pergunta passa pelo daemon como a do `sandmux chat` sem terminal, com o texto de sistema no campo `system` do `ask` (protocolo 21). Ela roda no mesmo confinamento da aba de chat, com a chave trocada no gateway.

A pasta enviada é a de trabalho da aba, não a pasta atual do shell, que a TUI não enxerga.

## Pergunta sem aba

Sem terminal, `sandmux chat` pede ao daemon a mensagem `ask` do protocolo (desde o protocolo 20; o texto de sistema, desde o 21). O daemon então:

1. cria uma pasta temporária de chat com a pergunta;
2. roda o `sandmux-chat --once` no mesmo confinamento da aba, com a saída em pipe;
3. devolve a resposta e apaga a pasta.

O pedido passa pelo daemon porque só lá o gateway alcança o cofre para trocar a chave.
