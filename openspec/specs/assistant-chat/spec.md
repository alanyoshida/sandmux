# assistant-chat Specification

## Purpose
Define o assistente de chat: o modal da TUI, a aba de chat e o programa sandmux-chat confinado, os modelos e provedores, a conversa guardada na pasta da aba, a chave do cofre trocada no tráfego e o comando sandmux chat.

## Requirements

### Requirement: Modal de chat
Na TUI, a tecla de prefixo seguida de `i` MUST abrir o modal de chat, com um campo de mensagem e um campo de modelo. O modelo MUST começar com o valor de `[assistant] chat-model`. Tab MUST alternar entre os campos; no campo de modelo, as setas MUST passar pelos modelos de `[assistant] models` e pelo padrão. Enter com uma mensagem não vazia MUST criar uma aba de chat com essa mensagem e esse modelo e torná-la ativa; Esc MUST fechar o modal sem criar nada. A ação MUST se chamar `chat` em `[tui.keys]`.

#### Scenario: Pergunta enviada
- **WHEN** `[assistant] chat-model` é `openrouter/deepseek/deepseek-v4.1-flash-1017`, o usuário digita Ctrl+b i, escreve "o que é um inode?" e aperta Enter
- **THEN** uma aba de chat nova fica ativa, com a pergunta e o modelo `openrouter/deepseek/deepseek-v4.1-flash-1017`

#### Scenario: Sem modelo
- **WHEN** a configuração não tem `chat-model` e o usuário aperta Enter com o campo de modelo vazio
- **THEN** o modal continua aberto e mostra que falta escolher um modelo

#### Scenario: Atalho trocado
- **WHEN** `[tui.keys]` tem `chat = "y"`
- **THEN** Ctrl+b y abre o modal de chat

### Requirement: Nome do modelo e provedores
O modelo MUST ser escrito `<provedor>/<modelo>`. Os provedores conhecidos MUST ser os do cenário "Provedores conhecidos", cada um com a URL base da API compatível com a da OpenAI. `[assistant.provider.<nome>]` com `url` MUST cadastrar outro provedor com a API compatível com a da OpenAI. Um provedor desconhecido MUST ser recusado com a lista dos disponíveis.

#### Scenario: Provedores conhecidos
- **WHEN** o modelo começa por um provedor conhecido
- **THEN** a URL base é a da tabela: `openrouter` `https://openrouter.ai/api/v1`, `anthropic` `https://api.anthropic.com/v1`, `openai` `https://api.openai.com/v1`, `gemini` `https://generativelanguage.googleapis.com/v1beta/openai`, `deepseek` `https://api.deepseek.com/v1`, `mistral` `https://api.mistral.ai/v1`, `groq` `https://api.groq.com/openai/v1` e `xai` `https://api.x.ai/v1`

#### Scenario: Modelo do OpenRouter
- **WHEN** o modelo é `openrouter/anthropic/claude-sonnet-5`
- **THEN** os pedidos vão a `https://openrouter.ai/api/v1/chat/completions` com o modelo `anthropic/claude-sonnet-5`

#### Scenario: Provedor próprio
- **WHEN** a configuração tem `[assistant.provider.local]` com `url = "http://host.sandmux.internal:11434/v1"` e o modelo é `local/llama3`
- **THEN** os pedidos vão a `http://host.sandmux.internal:11434/v1/chat/completions` com o modelo `llama3`

#### Scenario: Provedor desconhecido
- **WHEN** o modelo é `nada/x`
- **THEN** o modal recusa e lista os provedores disponíveis

### Requirement: Aba de chat
A aba de chat MUST ser uma sessão do daemon que roda o programa `sandmux-chat` num PTY, e MUST aparecer em `sandmux ls` e na barra de abas como as outras, com o nome `chat:<modelo>`. O programa MUST mostrar a conversa, o modelo em uso, uma barra de texto e um botão "Enviar". Enter ou o clique no botão MUST enviar o texto da barra; Ctrl+j MUST quebrar a linha. A resposta MUST aparecer aos poucos, conforme chega. `/modelo <nome>` na barra MUST trocar o modelo das mensagens seguintes. Um erro do provedor MUST aparecer na conversa, com o status e a mensagem dele, e a conversa MUST continuar utilizável.

#### Scenario: Conversa contínua
- **WHEN** a primeira resposta chegou e o usuário escreve outra pergunta na barra e clica em "Enviar"
- **THEN** o modelo recebe as mensagens anteriores e a nova, e a resposta aparece abaixo delas

#### Scenario: Chave recusada
- **WHEN** o provedor responde 401
- **THEN** a conversa mostra o status 401 e a mensagem do provedor, e a barra continua aceitando texto

### Requirement: Confinamento da aba de chat
A sessão de chat MUST rodar no sandbox, com a rede no modo só inferência mais o host de cada provedor de `[assistant.provider.<nome>]`, qualquer que seja o modo das camadas; as regras de bloqueio das camadas MUST valer. A sessão MUST NOT receber Docker privado, agente SSH, MCP de banco nem acesso ao repositório. A pasta de trabalho MUST ser uma pasta privada da aba, em `$XDG_STATE_HOME/sandmux/chat/<id>`, criada com permissão só do dono. O programa MUST receber a chave falsa do provedor, e o valor real MUST NOT entrar no sandbox.

#### Scenario: Rede da aba de chat
- **WHEN** a configuração global tem `network = "aberta"` e o usuário abre uma aba de chat
- **THEN** a sessão de chat roda no modo só inferência, e um pedido dela a `example.com` é bloqueado

#### Scenario: Chave real fora do sandbox
- **WHEN** o cofre tem o segredo `openrouter` e a aba de chat conversa com um modelo do OpenRouter
- **THEN** o programa manda o valor falso, o OpenRouter recebe o real, e o real não aparece em nenhum arquivo nem variável do sandbox

### Requirement: Conversa guardada
O programa MUST guardar a conversa em `conversa.json` na pasta da aba, com o modelo e as mensagens, a cada mensagem enviada ou resposta terminada. Ao iniciar, o programa MUST carregar a conversa da pasta; se a última mensagem for do usuário e não tiver resposta, MUST enviá-la. Fechar a aba de chat MUST apagar a pasta dela. A conversa MUST NOT ir para a lista de sessões do daemon.

#### Scenario: Reinício do daemon
- **WHEN** existe uma aba de chat com duas perguntas respondidas e o usuário roda `sandmux daemon restart`
- **THEN** a aba de chat volta com as duas perguntas e as respostas, sem enviar nada de novo

#### Scenario: Aba fechada
- **WHEN** o usuário fecha a aba de chat
- **THEN** a pasta da aba e o `conversa.json` são apagados

### Requirement: Cofre e chave do provedor
Com o cofre trancado, o modal de chat MUST abrir o desbloqueio antes de criar a aba, e criá-la depois do desbloqueio. Sem segredo no cofre para o host do provedor, o modal MUST recusar com o comando que cadastra a chave, a menos que o provedor seja de `[assistant.provider.<nome>]`, que pode não ter chave.

#### Scenario: Cofre trancado
- **WHEN** o cofre está trancado e o usuário envia uma pergunta pelo modal
- **THEN** o modal de desbloqueio abre, e a aba de chat é criada depois da senha certa

#### Scenario: Provedor sem chave
- **WHEN** o cofre não tem segredo para `api.openai.com` e o modelo é `openai/gpt-5`
- **THEN** o modal recusa e mostra `sandmux vault add openai --domain api.openai.com`

### Requirement: Comando chat
`sandmux chat [--model M] [TEXTO]` MUST usar o modelo `M` ou, sem ele, `[assistant] chat-model`. Com terminal, MUST criar uma aba de chat no daemon, com o texto como primeira mensagem quando houver, e anexar. Sem terminal, MUST exigir o texto, enviar a pergunta numa execução confinada como a da aba, imprimir a resposta na saída padrão e terminar, sem guardar a conversa; um erro do provedor MUST terminar com código diferente de zero.

#### Scenario: Pergunta num script
- **WHEN** o usuário roda `sandmux chat --model openrouter/deepseek/deepseek-v4.1-flash-1017 "responda só 42" | cat`
- **THEN** a resposta do modelo sai na saída padrão, o comando termina com código 0 e nenhuma pasta de chat fica em `$XDG_STATE_HOME/sandmux/chat`
