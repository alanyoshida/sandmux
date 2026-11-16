# Antigravity CLI

> English: [en/agy.md](en/agy.md)

O preset `agy` roda o Antigravity CLI do Google instalado na sua máquina dentro do sandbox, como o opencode, o hermes, o Claude Code e o oh-my-pi.

```sh
sandmux run agy
```

## Sem perguntas

O `agy` roda com `--dangerously-skip-permissions`, sem pedir confirmação para as ferramentas: o sandbox é a barreira. A lista de comandos bloqueados do próprio `agy` e as regras de negação que você escreveu continuam valendo. `harness-asks = true` na configuração devolve as perguntas ([config.md](config.md#perguntas-do-harness)).

## Instalação

O sandmux usa o binário `~/.local/bin/agy` do host, em somente leitura. Sem o binário, o `run` recusa e diz onde ele deveria estar.

A atualização automática fica desligada dentro do sandbox, pela variável `AGY_CLI_DISABLE_AUTO_UPDATE=true`. Para atualizar, rode `sandmux update agy`, que roda o `agy update` fora do sandbox ([sessions.md](sessions.md#atualizar-o-harness)).

## Chaves

O caminho recomendado é a chave da API do Gemini no cofre:

```sh
sandmux vault add gemini --domain generativelanguage.googleapis.com
```

Com a chave no cofre, o `agy` recebe `GEMINI_API_KEY` com o valor falso e é lançado com `--settings '{"modelProvider":"gemini"}'`, que vale só para aquela execução e não muda o seu `settings.json`. O `agy` manda a chave no cabeçalho `x-goog-api-key` para `generativelanguage.googleapis.com`, e o gateway troca o falso pelo real. O mesmo vale para `--pass-env GEMINI_API_KEY`, que entrega a chave real ao agente. As chaves dos outros provedores com segredo no cofre também entram por variável, como no opencode e no hermes.

Sem a chave do Gemini, o `agy` usa o login do Google ([abaixo](#login-do-google)).

## Login do Google

O token do login do Google nunca fica no sandbox. Ele vai para o cofre, como `agy:access` (o token de acesso) e `agy:refresh` (o de renovação), e o `agy` confinado recebe valores falsos com o mesmo formato (`ya29.sandmux…` e `1//sandmux…`). O gateway troca o falso pelo real:

- o de acesso, nos pedidos a qualquer `*.googleapis.com`;
- o de renovação, no corpo do pedido a `oauth2.googleapis.com/token`.

As abas do `agy` decifram `*.googleapis.com` para isso. As outras abas não decifram esses hosts por causa do login, e o falso, nelas, segue falso.

**Login do host:** se você já entrou no `agy` fora do sandmux, a importação leva esse login para o cofre e grava no HOME sintético o arquivo do token com os falsos. Você não entra de novo. O Google não troca o token de renovação a cada renovação, e por isso o `agy` de fora e o de dentro usam o mesmo login sem um derrubar o outro.

- **Cofre fechado:** o cofre precisa estar aberto na importação. Fechado, ou sem cofre, a importação traz o resto e avisa. Depois de abrir o cofre (`sandmux vault unlock`, ou `sandmux vault init`), rode `sandmux import agy --overwrite`.
- **Outra conta no host:** se você entrar com outra conta no `agy` do host, `sandmux import agy --overwrite` leva o login novo ao cofre. Os falsos do HOME sintético continuam os mesmos.
- **Sair da conta no host** pode revogar o token, e então o `agy` confinado para de renovar. Entre de novo no host e rode `sandmux import agy --overwrite`, ou entre pelo sandmux (abaixo).

**Login pelo sandmux:** sem login no host, entre pelo próprio `agy` confinado:

1. Escolha "Google OAuth". O `agy` mostra um endereço.
2. Abra o endereço no navegador do host e autorize.
3. A página mostra um código. Cole o código no `agy` (o retorno automático não alcança o sandbox).

Na resposta do login e de cada renovação, o gateway guarda os tokens reais no cofre e entrega os falsos ao `agy`. O cofre precisa estar aberto. Sem cofre que aceite, a resposta é recusada, e nenhum token real chega ao sandbox. Só nas abas do `agy` esse endpoint é capturado: o `gcloud` num shell confinado, que usa o mesmo endpoint, recebe a resposta como ela veio.

**Login antigo:** um login feito dentro do sandbox antes desta versão deixou o token real no HOME sintético. O daemon o leva sozinho ao cofre e deixa os falsos no arquivo, como faz com as [chaves gravadas dentro do sandbox](vault.md#chaves-gravadas-dentro-do-sandbox). Com o cofre trancado, ele espera o cofre abrir, e o `sandmux doctor` mostra o arquivo.

**O que o token permite:** o login pede o escopo `cloud-platform`, além do `aicode` e dos de perfil. Como o token real vai a todo `*.googleapis.com`, o agente da aba do `agy` pode usar o falso para agir no seu Google Cloud pelo gateway. Cada uso fica no registro de rede com o nome `agy:access`. O `id_token` da resposta de login, que o `agy` lê para saber o e-mail da conta, chega ao sandbox como veio.

**Modo só inferência:** o catálogo de inferência tem `oauth2.googleapis.com` (a renovação) e os hosts do modelo do `agy` (`aicode.googleapis.com`, `cloudcode-pa.googleapis.com` e `daily-cloudcode-pa.googleapis.com`). O primeiro login pelo sandmux precisa desses hosts liberados.

## Importação

Na primeira execução, o sandmux pergunta se importa a configuração que você já tem. Ele copia:

- de `~/.gemini/antigravity-cli`, o `settings.json` e o `keybindings.json`;
- de `~/.gemini/config`, a configuração compartilhada com o app: `config.json`, `mcp_config.json`, `hooks.json`, as regras globais (`AGENTS.md`, `GEMINI.md`, `skills.json`, `rules.json`, `plugins.json`, `agents.json`) e as pastas `hooks/`, `plugins/`, `skills/`, `agents/`, `rules/` e `workflows/`;
- as regras globais `~/.gemini/GEMINI.md` e `~/.gemini/AGENTS.md`.

Ficam de fora:

- os segredos de plugins (`~/.config/antigravity/plugin-secrets.json`);
- os projetos (`~/.gemini/config/projects` e o `cache/projects.json`), que guardam caminhos e identificadores do host;
- as conversas, o histórico, os registros, os caches e os bancos.

O token do login do Google (`antigravity-oauth-token`) não é copiado como está: com o cofre aberto, ele vai para o cofre, e o HOME sintético recebe o arquivo com os falsos ([Login do Google](#login-do-google)).

Um segredo no que é copiado, como uma chave no `env` de um MCP, é trocado por um valor falso, como nos outros harnesses ([vault.md](vault.md)).
