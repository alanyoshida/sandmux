## Context

O sandmux já tem tudo de que uma conversa com um modelo precisa, mas espalhado:

- o cofre guarda as chaves dos provedores;
- o gateway de cada sandbox decifra o tráfego dos domínios com segredo e troca o valor falso pelo real;
- o daemon mantém sessões em PTY que sobrevivem ao fechamento do terminal e voltam depois de um reboot;
- a TUI mostra cada sessão numa aba, com mouse, rolagem e vários clientes.

O que falta é um programa de conversa e o caminho do modal até ele. As decisões 2.5 e 2.7 do roadmap valem aqui:

- nenhum segredo real dentro do sandbox;
- modelo padrão do chat na configuração, com troca no modal.

O usuário decidiu, em 2026-11-14, que a conversa fica em disco e volta depois de um reinício.

## Goals / Non-Goals

**Goals:**
- Modal de chat na TUI e aba de chat com conversa contínua, streaming, barra de texto e botão "Enviar".
- A chave real nunca entra no sandbox, nem no programa de chat.
- A conversa volta depois de `sandmux daemon restart` e de um reboot, e some quando a aba é fechada.
- Uma pergunta sem terminal (`sandmux chat "..." | cat`), que o change 22 reaproveita para o modal de comando.

**Non-Goals:**
- Ferramentas para o modelo (ler arquivos, rodar comandos). A aba de chat é só conversa.
- Renderização de Markdown. O texto aparece como veio, com quebra de linha pela largura.
- APIs que não sejam compatíveis com a da OpenAI. Os oito provedores conhecidos têm esse endpoint.
- OAuth e assinatura dos provedores (depois da v1).
- Lista de modelos baixada do provedor. O modal oferece os da configuração e aceita qualquer nome.

## Decisions

### A aba de chat é uma sessão do daemon com um programa confinado

A aba de chat é uma sessão comum do daemon, com o harness interno `chat`. Ela roda o binário `sandmux-chat` num PTY, dentro do sandbox. O binário fica ao lado do `sandmux-run` e entra em somente leitura em `/run/sandmux/bin/sandmux-chat`, como o `sandmux-db-mcp`.

Com isso a aba herda, sem código novo:

- a barra de abas e a lista;
- o anexo de vários clientes, a rolagem e a seleção;
- o fechamento e a restauração depois de um reboot;
- o registro de rede e a troca de chave no gateway.

**Alternativas consideradas:**
- **Tipo de aba nativo da TUI, com o daemon fazendo o pedido HTTP com a chave real.** Exigiria um protocolo novo para mensagens, uma renderização própria, o sincronismo entre clientes e a persistência à parte. Também tiraria o pedido do registro de rede do sandbox.
- **Programa de chat fora do sandbox.** Precisaria da chave real. Contraria a regra 2.5.

O harness `chat` não entra na lista do `sandmux run` nem no modal de aba nova. A aba nasce pelo modal de chat ou pelo `sandmux chat`.

### O programa `sandmux-chat`

- **Interface:** ratatui com crossterm sobre o PTY, em tela alternativa e com mouse SGR. A TUI do sandmux já repassa ao harness da aba o mouse que ele pediu.
- **Layout:**
  - no topo, uma linha com o modelo;
  - no meio, a conversa, com rolagem pela roda do mouse e por PgUp/PgDn;
  - embaixo, a barra de texto e o botão `[ Enviar ]`.
- **Teclas:** Enter envia e Ctrl+j quebra a linha. `/modelo <nome>` troca o modelo.
- **Pedido:** `POST <url>/chat/completions` com `stream: true`.
  - O cliente é `hyper` com `tokio-rustls`. As raízes vêm do arquivo de `SSL_CERT_FILE`, que no sandbox já traz a CA do lançamento; sem a variável, valem as do sistema.
  - Para `http://`, é a conexão TCP crua.
  - A resposta é lida como SSE: linhas `data:`, com o texto em `choices[0].delta.content`, até `[DONE]`.
- **Erro do provedor:** status diferente de 2xx vira uma linha de erro na conversa, com o status e o `error.message` do corpo, quando houver. Essa linha não é guardada.
- **Modo `--once`:**
  - lê a conversa da pasta;
  - faz o pedido sem streaming;
  - imprime só o texto da resposta na saída padrão;
  - termina com 0, ou com 1 e a mensagem de erro na saída de erro.

### Provedores e chaves

A tabela de provedores fica em `sandmux_policy::assistant`, com o nome, a URL base e a variável da chave. As variáveis são as de `PROVIDERS`. O change 22 usa a mesma tabela.

O modelo `p/resto` é dividido na primeira barra:
- `p` é um provedor conhecido ou um de `[assistant.provider.<p>]`;
- `resto` é o modelo enviado ao provedor.

O daemon entrega ao programa a variável `SANDMUX_CHAT_PROVIDERS`, um JSON com `{name, url, env}` de cada provedor:
- **provedores conhecidos:** `env` é a variável do provedor. A chave falsa chega pelo mesmo caminho dos harnesses: `chat` entra em `FAKE_KEY_HARNESSES`.
- **provedores próprios:** se o cofre tem segredo dono do host da URL, o daemon põe o falso numa variável `SANDMUX_CHAT_KEY_<NOME>` e a cita em `env`. Sem segredo, o pedido sai sem `Authorization`.

A troca do falso pelo real é a do gateway, já existente. Para um host `http://`, não há decifração nem troca. Por isso um provedor próprio com chave precisa de HTTPS; a documentação diz isso.

### Confinamento e rede

O lançamento da aba de chat resolve as camadas com a configuração do momento (pasta = a pasta da aba, sem perfil nomeado) e depois ajusta o resultado:

- **rede:** força `Inferencia`. A regra de cada host de provedor próprio entra como regra de permissão com a origem `assistente`, que o filtro do modo só inferência mantém. As regras de bloqueio continuam valendo.
- **sem Docker privado:** o daemon não cria o socket para `chat`.
- **sem agente SSH nem MCP de banco:** as listas vazias.
- **sem repositório:** `repo` desligado.
- **pastas extras e `pass-env` das camadas:** ignorados. A aba de chat não precisa delas e não deve ganhar acesso por elas.
- **tetos de memória e processos:** os padrões.

### Pasta da aba e conversa

O cliente cria a pasta da aba:

- **local:** `chat/<aleatório>` dentro de `state_dir()`, com modo 0700;
- **conteúdo inicial:** um `conversa.json` com o modelo e a primeira mensagem.

Depois o cliente pede ao daemon a sessão com o harness `chat` e essa pasta como pasta de trabalho. A mensagem não passa pelos argumentos, então não vai para a lista de sessões.

O formato do arquivo é `{"versao": 1, "modelo": "...", "mensagens": [{"papel": "user"|"assistant", "texto": "..."}]}`.

**Gravação:** o programa grava num arquivo temporário e renomeia. Isso acontece a cada mensagem enviada e a cada resposta completa. Uma resposta interrompida não é gravada: na volta, a última mensagem é do usuário, e o programa a reenvia.

**Ao fechar a aba**, o daemon remove a sessão e apaga a pasta. Ele só apaga se o caminho canônico estiver dentro de `state_dir()/chat` e não for link.

**Na restauração:**
- com o cofre existente e trancado, a sessão fica "aguardando cofre", como o gateway do hermes;
- se a pasta sumiu, a sessão fica parada, como já acontece com qualquer pasta de trabalho apagada.

### Modal de chat

O modal `Modal::Chat` tem dois campos, a mensagem e o modelo. A mensagem do modal é uma linha só, porque a TUI recebe Ctrl+j como Enter; as linhas seguintes vão pela barra da aba, que aceita Ctrl+j.

**Enter:**
1. confere o provedor e o nome;
2. pergunta ao daemon se o cofre tem segredo para o host do provedor, com o pedido novo `Msg::ChatCheck`;
3. se o cofre está trancado, guarda o pedido e abre o `Modal::Unlock`, e o desbloqueio bem-sucedido cria a aba;
4. sem segredo para um provedor conhecido, mostra no modal `sandmux vault add <provedor> --domain <host>`;
5. cria a pasta e a sessão, e ativa a aba.

### `sandmux chat` sem terminal

O pedido novo `Msg::Ask { model, text }` faz o daemon:
1. criar uma pasta temporária de chat com a pergunta;
2. lançar o `sandmux-chat --once` no mesmo confinamento da aba, com a saída em pipe e não em PTY;
3. esperar o fim e apagar a pasta;
4. responder `Msg::Answer { text }` ou `Msg::Error`.

O pedido passa pelo daemon porque só lá o gateway tem acesso ao cofre para trocar a chave.

O change 22 usa o mesmo pedido, com outro texto de sistema.

### Protocolo 20

`Msg::ChatCheck`, `Msg::ChatStatus`, `Msg::Ask` e `Msg::Answer`, além do harness `chat` nas `LaunchOptions`. Um daemon antigo não conheceria o harness nem os pedidos.

## Risks / Trade-offs

- **[A conversa fica em disco, contra a regra do histórico de tela]** A regra vale para a saída dos harnesses. A conversa do chat é exceção por decisão do usuário. A pasta é privada (0700) e some quando a aba é fechada.
- **[Provedor próprio por `http://` não tem troca de chave]** Só HTTPS é decifrado. Um servidor local sem chave funciona; com chave, precisa de HTTPS. A documentação avisa.
- **[Endpoints compatíveis com a OpenAI variam em detalhes]** Anthropic e Gemini aceitam o formato, mas podem recusar campos. O programa manda só `model`, `messages` e `stream`.
- **[Reenvio da última pergunta na volta]** Se o daemon cair no meio da resposta, a pergunta é refeita ao voltar. Custa um pedido; a alternativa, guardar a resposta parcial, mostraria texto cortado como se fosse completo.
- **[Linhas longas e caracteres largos]** A quebra de linha usa a largura Unicode, já presente pelo ratatui.
