# Cofre de segredos

O cofre guarda segredos fora do sandbox: chaves de API, tokens e os valores de `--pass-env` das sessões. Ele fica num arquivo cifrado com a sua senha, e o daemon só o abre na memória depois que você digita a senha. O agente só vê valores falsos; o gateway troca o falso pelo real no tráfego, e só nos pedidos ao dono do segredo.

## Criar e desbloquear

```bash
sandmux vault init      # cria o cofre, pedindo a senha duas vezes
sandmux vault status    # existe? desbloqueado? quantos segredos
sandmux vault unlock    # desbloqueia no daemon
sandmux vault lock      # tranca e apaga os segredos da memória do daemon
sandmux vault passwd    # troca a senha
```

O daemon sobe no login com o cofre trancado. Para desbloquear:

- **Na TUI:** ao abrir, ela mostra um modal pedindo a senha, sem mostrar o que é digitado. Enter desbloqueia; Esc deixa trancado.
- **Fora dela:** `sandmux vault unlock` pede a senha no terminal, sem eco.
- **Pelo keyring do sistema:** opcional; veja [Keyring](#keyring).

Sem terminal (num script), os comandos leem a senha e os valores da entrada padrão, uma linha por pergunta.

### Keyring

O keyring do sistema pode guardar a senha do cofre, para o daemon abrir o cofre sozinho. Funciona com qualquer keyring do Secret Service (`org.freedesktop.secrets`): gnome-keyring, KWallet, KeePassXC. Vem desligado.

```bash
sandmux vault keyring on    # pede a senha, confere com o cofre e a guarda no keyring
sandmux vault keyring       # ligado ou desligado, e, ligado, se o keyring responde
sandmux vault keyring off   # apaga a senha do keyring
```

- **Só a senha** vai para o keyring. O cofre continua no arquivo cifrado, e os segredos nunca passam pelo keyring.
- **Quando o daemon usa:** ao subir, antes de atender o primeiro cliente, e quando um cliente pergunta o estado do cofre trancado (a TUI faz isso ao abrir), no máximo a cada 5 segundos. Abrir pelo keyring tem o efeito do `vault unlock`: as abas que esperavam o cofre sobem. Se o keyring não responde em 3 segundos, não tem a senha ou tem uma senha que não abre mais o cofre, o cofre fica trancado e a senha é pedida como antes. O registro do daemon diz o motivo, sem a senha.
- **Desligado,** o sandmux não consulta o keyring. O marcador de ligado é o arquivo `vault.keyring` na pasta de dados, sem nada secreto dentro. No keyring, o item tem o serviço `sandmux` e o usuário igual à pasta de dados.
- **Troca de senha:** `sandmux vault passwd` regrava a senha no keyring ligado.
- **O custo:** com o keyring ligado, qualquer programa seu fora do sandbox pode ler a senha enquanto o keyring da sessão está aberto. Sem ele, a senha só existe na sua cabeça e na memória do daemon. O sandbox não alcança o D-Bus, então o agente não chega ao keyring.
- **Antes do login,** o keyring do usuário está fechado, e o cofre não abre sozinho.

## Segredos

```bash
sandmux vault add openrouter --domain openrouter.ai   # pede o valor, sem eco
sandmux vault list                                    # nomes, domínios e valores falsos
sandmux vault set openrouter                          # troca só o valor (chave vencida)
sandmux vault remove openrouter
```

Cada segredo tem um nome, o valor e, se quiser, os domínios donos (padrões como os da [rede](network.md)), que dizem para onde o valor pode ir quando a troca no tráfego existir. Com o cofre desbloqueado, o gateway [decifra o TLS](network.md#decifração-do-tls) desses domínios e registra o método e o caminho de cada pedido. O valor nunca vai num argumento da linha de comando, porque argumentos ficam visíveis para outros processos.

### Trocar uma chave que venceu

`sandmux vault set <nome>` pede só o valor novo, sem eco. O falso, os domínios e as ferramentas continuam os mesmos, então nenhum arquivo do harness precisa mudar, nem os que a importação ou o `doctor --fix` gravaram com o falso. O gateway pede os segredos ao daemon a cada conexão, e o valor novo vale na próxima conexão, sem reiniciar a aba (uma conexão já aberta segue com o valor antigo até fechar; Ctrl+b `r` força).

Gravar de novo com `sandmux vault add` e o mesmo nome também troca o valor, mas gera um falso novo: um arquivo do harness com o falso antigo deixa de funcionar. Uma chave SSH se troca gravando a chave nova (`sandmux vault add-ssh`), e uma conexão de banco, com `sandmux db add`.

## Pela TUI

A seção Cofre da página de configurações (Ctrl+b `e`) mostra o estado do cofre e a lista de segredos, com o nome, o tipo (token ou SSH) e os domínios. Os valores nunca aparecem. Tab ou → entra na lista, e as teclas fazem o mesmo que os subcomandos:

| Tecla | Ação |
|---|---|
| `n` | Cria o cofre (senha e confirmação) |
| `u` / `l` | Desbloqueia / tranca |
| `p` | Troca a senha |
| `k` | Liga o [keyring](#keyring), pedindo a senha, ou desliga |
| `a` | Acrescenta um segredo: nome, valor e domínios separados por vírgula (`localhost` vira `host.sandmux.internal`) |
| `s` | Acrescenta uma chave SSH: nome, arquivo, passphrase (vazia se não tem) e servidores |
| `v` | Troca só o valor do segredo escolhido, mantendo o falso e os domínios (uma chave que venceu) |
| `d` | Troca os domínios do segredo escolhido, sem pedir o valor de novo |
| `x` | Apaga o segredo escolhido, depois de `s` para confirmar |
| `r` | Relê o cofre |
| `g` | Abre o [grafo do cofre](#grafo-do-cofre) |

Senhas e valores aparecem como `•` e são apagados da memória da TUI depois de enviados ao daemon. As conexões de banco, que o grafo só mostra, e a importação de chaves continuam na linha de comando.

### Grafo do cofre

Na seção Cofre, `g` abre o grafo do cofre em tela cheia. Ele mostra, a partir de um domínio, os segredos que podem ir para ele, e a partir de um segredo, todos os domínios donos. Os valores nunca aparecem.

- **À esquerda, a lista:** um item para cada domínio dono, com o número de segredos. Cada texto de domínio é um item: `github.com` e `*.github.com` ficam separados, porque casam com pedidos diferentes. No fim vêm "Sem domínio", com os segredos sem dono (o valor deles não é trocado no tráfego), e "Bancos", com as [conexões de banco](#conexões-de-banco). Cada um desses grupos só aparece quando tem algum item.
- **No meio, o grafo:** o domínio escolhido no centro e os segredos dele em volta, cada um ligado por uma aresta. A marca diz o tipo: `●` token, `◆` chave SSH, `■` conexão de banco, `▲` identidade de cloud. Um segredo com outros donos leva `+N` depois do nome (N é o número dos outros donos) e aparece também no grafo de cada um deles. Com mais de 8 segredos, eles se dividem em anéis: 8 no primeiro, 12 no segundo, 16 no terceiro.
- **À direita, os detalhes** do segredo em foco: o tipo, o valor falso (ou a impressão digital de uma chave SSH), as ferramentas e todos os domínios donos. De uma conexão de banco: o tipo, o servidor, a porta, o banco e o usuário, sem a senha.

**Zoom:** o texto do terminal não muda de tamanho. O zoom, de 0.5x a 2.0x, muda a distância entre os nós e quanto cada um mostra: abaixo de 0.7x, só a marca; até 1.3x, a marca e o nome; daí em diante, um cartão com o nome, o tipo e as ferramentas. O rodapé mostra o zoom atual.

| Tecla | Ação |
|---|---|
| ↑ ↓ | Escolhem o domínio; com o foco num segredo, o anterior e o seguinte |
| Tab ou → | Levam o foco aos segredos do domínio; Tab de novo passa ao seguinte |
| ← | Devolve o foco à lista |
| Enter | Abre e fecha os detalhes, quando eles não cabem ao lado |
| `o` | Vai ao dono seguinte do segredo em foco, com ele ainda em foco |
| `+` `-` | Zoom |
| `h` `j` `k` `l` | Movem o grafo para a esquerda, para baixo, para cima e para a direita |
| `0` | Volta ao centro e ao zoom 1.0x |
| `/` | Filtra a lista pelo nome do domínio ou de um segredo (Enter mantém, Esc limpa) |
| `a` `s` | Acrescentam um segredo ou uma chave SSH, com o domínio escolhido já preenchido |
| `d` `v` `x` | Trocam os domínios, trocam o valor ou apagam o segredo em foco |
| `u` | Desbloqueia o cofre, quando ele está trancado |
| `r` | Relê o cofre |
| Esc | Limpa o filtro, devolve o foco à lista ou volta à seção Cofre |

- **Mouse:** a roda sobre o grafo muda o zoom, arrastar move o grafo, um clique na lista escolhe o domínio e um clique num nó põe o foco nele.
- **Edição:** os formulários são os da seção Cofre, e o grafo é redesenhado depois de cada mudança. As teclas `l`, `k`, `p` e `n` da seção não valem nesta tela. Uma conexão de banco é só de leitura aqui: ela se gerencia por `sandmux db`.
- **Largura:** com 100 colunas ou mais, lista, grafo e detalhes ficam lado a lado. De 70 a 99, os detalhes abrem com Enter, numa caixa sobre o grafo. Com menos de 70, a tela mostra só a lista.
- **Fonte sem Braille:** as arestas usam os pontos do Braille. Se eles aparecerem como quadrados no seu terminal, troque para meio-bloco, em "Arestas do grafo" na seção Aparência ou no arquivo:

```toml
[tui]
graph-edges = "blocos"
```

- **Limite:** as posições são fixas, em anéis. Com nomes longos e muitos segredos num domínio, dois nomes podem se sobrepor; o zoom e o movimento separam.

## Troca da chave no tráfego

Cada segredo tem um valor falso (`sandmux-fake-` e 32 caracteres): o que a importação pôs no lugar, ou um gerado no `sandmux vault add`. O `sandmux vault list` mostra o falso, que é o que o agente vê de qualquer jeito.

- **No sandbox:** nas sessões do opencode, do hermes, do oh-my-pi e do Antigravity CLI, com o cofre desbloqueado, cada provedor de inferência com segredo no cofre entra sozinho no ambiente com o valor falso: `OPENROUTER_API_KEY` (`openrouter.ai`), `ANTHROPIC_API_KEY` (`api.anthropic.com`), `OPENAI_API_KEY` (`api.openai.com`), `GEMINI_API_KEY` (`generativelanguage.googleapis.com`), `DEEPSEEK_API_KEY` (`api.deepseek.com`), `MISTRAL_API_KEY` (`api.mistral.ai`), `GROQ_API_KEY` (`api.groq.com`) e `XAI_API_KEY` (`api.x.ai`). No opencode, a configuração injetada aponta a chave do provedor para a variável; no hermes, o falso vai no `~/.hermes/.env` daquela execução; no Antigravity CLI, a `GEMINI_API_KEY` faz o `agy` usar a chave no lugar do login do Google ([agy.md](agy.md#chaves)). Um `--pass-env` da mesma variável é ignorado, com aviso: vale o falso, e o valor real não entra.
- **No gateway:** os domínios donos dos segredos são [decifrados](network.md#decifração-do-tls). A cada conexão decifrada, o gateway pede os segredos ao daemon pelo socket privado e troca, nos cabeçalhos, o falso pelo real, só quando o domínio do pedido é dono do segredo. O falso mandado a outro destino segue falso. O corpo nunca é alterado.
- **Cofre trancado:** nada é trocado; os pedidos saem com o falso e o provedor recusa. Desbloqueie e repita.
- **Logins capturados:** os tokens dos logins que o gateway captura (`claude:*`, do Claude Code, e `agy:*`, do Antigravity CLI) só valem nas abas do harness deles: as outras abas não decifram esses domínios por causa deles, e o falso segue falso ([claude.md](claude.md), [agy.md](agy.md#login-do-google)).
- **Registro:** a linha `https` de um pedido com troca tem o nome do segredo usado (`chave: openrouter` no `sandmux net` e no painel), nunca o valor.
- **Modo só inferência:** com o cofre desbloqueado e algum segredo de um provedor do catálogo, o modo `inferencia` libera só os provedores com segredo.

Para usar o OpenRouter sem a chave no sandbox:

```bash
sandmux vault add openrouter --domain openrouter.ai
sandmux run opencode
```

## Ferramentas

Um segredo pode servir a ferramentas de desenvolvimento, `gh`, `git`, `npm`, `pip`, `cargo` e `docker`, e aos canais do hermes, `telegram`, `discord`, `slack` e `slack-app` ([hermes.md](hermes.md#tokens-dos-canais)). Com o cofre desbloqueado, cada sessão recebe o valor falso já na configuração da ferramenta, e o gateway troca o falso pelo real no pedido ao domínio dono, como nas chaves de inferência.

```bash
sandmux vault add github --domain github.com                          # gh e git, pelo domínio
sandmux vault add npm-empresa --domain npm.empresa.com --tool npm
sandmux vault add registry --domain registry.empresa.com --tool docker
```

Sem `--tool`, os domínios conhecidos decidem: `github.com` leva a `gh` e `git`; `registry.npmjs.org` a `npm`; `pypi.org` a `pip`; `crates.io` a `cargo`; `ghcr.io`, `registry-1.docker.io` e `docker.io` a `docker`; `api.telegram.org` a `telegram`; `discord.com` a `discord`; e `slack.com` a `slack-app` quando o valor começa com `xapp-`, ou a `slack` nos demais. Sem `--domain`, um nome que contém `telegram`, `discord` ou `slack` traz os domínios do canal. O `sandmux vault list` mostra as ferramentas de cada segredo. Um segredo cadastrado antes ganha as ferramentas dos domínios conhecidos no primeiro desbloqueio.

O que entra em cada sessão, para cada domínio exato do segredo (um curinga como `*.empresa.com` não tem host para escrever na configuração; a troca no tráfego vale para ele, mas use o domínio exato para a ferramenta):

| Ferramenta | No sandbox |
|---|---|
| `gh` | `GH_TOKEN` com o falso |
| `git` | um ajudante de credenciais para `https://<domínio>` pelas variáveis `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_n` e `GIT_CONFIG_VALUE_n`, que responde usuário `sandmux` e o falso. As variáveis vêm antes do `.gitconfig` |
| `npm` | um `~/.npmrc` só daquela execução: o guardado no HOME sintético mais `//<domínio>/:_authToken=<falso>` |
| `pip` | `PIP_EXTRA_INDEX_URL=https://sandmux:<falso>@<domínio>/simple`, fora do `pypi.org` |
| `cargo` | `CARGO_REGISTRY_TOKEN` com o falso, para `crates.io` |
| `docker` | um `~/.docker/config.json` só daquela execução: o guardado mais `auths` do domínio. O [Docker privado](docker.md#registries-privados) troca o falso no `docker pull` |
| `telegram`, `discord`, `slack`, `slack-app` | `TELEGRAM_BOT_TOKEN`, `DISCORD_BOT_TOKEN`, `SLACK_BOT_TOKEN` e `SLACK_APP_TOKEN` com o falso, também no `~/.hermes/.env` daquela execução. O falso imita o formato do token: `0000000000:sandmux-fake-...`, `xoxb-sandmux-fake-...` e `xapp-sandmux-fake-...` |

Os arquivos guardados no HOME sintético não mudam: o `~/.npmrc` e o `~/.docker/config.json` da sessão são montados por cima e somem no fim. Um `docker login` dentro do sandbox grava o falso nesse arquivo, e o real nunca entra. Um `--pass-env GH_TOKEN` com segredo `gh` no cofre é ignorado, com aviso.

O git por HTTPS, o pip e os registries de contêiner mandam o usuário e a senha no `Authorization: Basic` (base64 de `usuário:senha`): o gateway abre o base64, troca o falso e fecha de novo. O falso também é trocado no caminho da URL (o Telegram leva o token em `/bot<token>/`) e nas mensagens de texto de um WebSocket do sandbox ao dono (o Discord manda o token no IDENTIFY). O corpo de um pedido HTTP nunca é alterado.

## Chaves SSH

O git por SSH usa o agente SSH do sandmux: a chave privada fica no cofre, e o daemon assina por ela. A chave nunca entra no sandbox.

```bash
sandmux vault add-ssh github --file ~/.ssh/id_ed25519 --server github.com   # pede a passphrase, se houver
sandmux run opencode --ssh-key github
```

- **Cadastro:** o `sandmux vault add-ssh` lê a chave privada OpenSSH (Ed25519, ECDSA P-256 ou P-384, ou RSA), pede a passphrase sem eco quando ela é cifrada e guarda a chave sem passphrase, cifrada no cofre, com os servidores dela (`--server`, domínios exatos, ao menos um). O `sandmux vault list` mostra `github  ssh  (github.com)  SHA256:...`, nunca a chave. Uma chave SSH não tem valor falso nem entra na troca de tokens.
- **Chaves da sessão:** a chave `ssh-keys` da [configuração](config.md) (num perfil ou no perfil do projeto) e o `--ssh-key <nome>` do `run` dizem quais chaves a sessão pode usar; `"*"` libera todas. Sem nenhuma, a sessão não tem agente. Um nome que não está no cofre aberto gera um aviso no lançamento.
- **No sandbox:** o agente fica em `/run/sandmux/ssh-agent.sock`, com `SSH_AUTH_SOCK` apontando para ele. Ele lista só as chaves escolhidas e assina só com elas; pedidos para adicionar, remover ou trancar chaves são recusados, e o RSA só assina com SHA-2 (`rsa-sha2-256` ou `rsa-sha2-512`). Com o cofre trancado, a lista fica vazia; um desbloqueio vale na hora, sem relançar.
- **Registro:** cada assinatura vira uma linha `ssh-sign` no [registro de rede](network.md#registro), com o nome e a impressão digital da chave (`assinatura ssh: github` no `sandmux net` e no painel).
- **Servidores:** o SSH de saída só chega aos servidores das chaves escolhidas, em qualquer porta ([network.md](network.md#como-o-gateway-decide)). No modo `custom`, o servidor também precisa estar liberado.
- **`known_hosts`:** a sessão recebe um `~/.ssh/known_hosts` só daquela execução, com o guardado no HOME sintético mais as linhas do `~/.ssh/known_hosts` do host para os servidores das chaves escolhidas (inclusive as com nome em hash). Sem essas linhas no host, o `ssh` pede a confirmação da chave do servidor na primeira conexão; conecte uma vez pelo host, ou confirme dentro da sessão.

Numa sessão com agente, `/etc/ssh/ssh_config.d` vira uma cópia dos arquivos `.conf` do sistema, do usuário do sandbox: sem isso, o `ssh` recusa os arquivos incluídos, que dentro do sandbox aparecem com o dono `nobody`.

Chaves em hardware (YubiKey) e agentes de terceiros (como o do 1Password) não são atendidos: o agente do sandmux só usa as chaves do cofre.

## Identidades de cloud

O cofre também guarda identidades de cloud (`sandmux vault add-cloud gcp <nome>`): o agente usa o `gcloud` e as bibliotecas do Google com um token falso, e o real fica fora do sandbox, com o daemon. Na TUI e no grafo, uma identidade aparece no domínio `*.googleapis.com`, só para leitura: `x` apaga. Detalhes em [cloud.md](cloud.md).

## Conexões de banco

O cofre também guarda conexões de PostgreSQL e MySQL (`sandmux db add`), com a senha cifrada, para o MCP de banco embutido. Detalhes em [database.md](database.md).

## Chaves da importação

A [importação](../README.md) troca cada segredo dos arquivos de um harness por um valor falso (`sandmux-fake-...`) e anota a troca num manifesto, sem o valor real. `sandmux vault import` percorre os manifestos e, para cada valor falso ainda sem segredo no cofre, mostra o harness, o arquivo, a chave e a linha do arquivo importado (com o falso como `<falso>`, para você reconhecer o provedor), e pede o valor real. Uma resposta vazia pula o item.

Ao digitar ou colar um segredo ou a senha do cofre, cada caractere aparece como `•`: o valor nunca aparece na tela. Backspace apaga um caractere e Ctrl+U apaga tudo.

### Chaves gravadas dentro do sandbox

Uma chave colada dentro do harness, como no "Connect provider" do opencode ou no assistente de configuração do hermes, fica em claro no HOME sintético, ao alcance do agente.

- **Busca:** o `sandmux doctor` procura credenciais reais nos arquivos de configuração (JSON, YAML, `.env`) que a importação copia, nos HOMEs sintéticos do opencode e do hermes, com as mesmas regras da importação. Ele lista cada uma (harness, arquivo, provedor, chave e linha), sem o valor.
- **`sandmux doctor --fix`:** leva essas chaves para o cofre, com o domínio descoberto como acima. O arquivo só é regravado com valores falsos depois de o cofre aceitar todas as chaves dele; o real não fica em nenhum arquivo do HOME sintético.
- **Pela TUI:** a seção Diagnóstico das configurações (Ctrl+b `D`) faz o mesmo com a tecla `f`. O domínio de cada chave vem num campo que dá para editar. Quando nada indica o domínio, o campo sugere o host da URL mais próxima da chave no arquivo ([tui.md](tui.md#diagnóstico)).
- **Depois:** reinicie as abas do harness, que pode ter a chave antiga na memória.
- **Login do Antigravity CLI:** o `doctor` também acha o token real de um login do Google feito dentro do sandbox (`~/.gemini/antigravity-cli/antigravity-oauth-token` do HOME do `agy`), e o `--fix` o leva ao cofre como `agy:access` e `agy:refresh` ([agy.md](agy.md#login-do-google)).

#### Movimento automático

O daemon faz o mesmo que o `doctor --fix` sozinho, sem perguntar:

- **Quando:** logo depois de o harness gravar um desses arquivos (o daemon observa as pastas deles), ao subir, a cada 60 segundos e logo depois de o cofre ser aberto.
- **Como:** com o cofre aberto e o domínio de cada chave descoberto como abaixo, cada chave vai ao cofre com um falso novo, e só depois o arquivo é regravado com os falsos. Se o cofre recusar uma chave, ou se o harness regravou o arquivo durante a varredura, o arquivo fica como estava. O registro do daemon diz o harness, o arquivo e o nome das chaves, nunca os valores.
- **Avisos:** a aba do harness mostra quantas chaves foram movidas desde o lançamento, e pede para reiniciar a aba, porque o harness ainda tem a real na memória. Ela também mostra quantas esperam o cofre (trancado ou ausente) e quantas esperam um domínio. As que esperam o cofre vão quando ele é aberto. As sem domínio ficam para o `sandmux doctor --fix` ou para a seção Diagnóstico da TUI, que perguntam. Os avisos aparecem no `sandmux ls`, na marca `✱` da aba, nos alertas da barra e na linha "chaves reais" do painel da TUI ([tui.md](tui.md)).

Sem cofre, nada troca o falso pelo real, e o harness recebe "chave inválida" do provedor. Por isso o relatório da importação e o `sandmux doctor` lembram de `sandmux vault init` e `sandmux vault import`. O segredo fica com o nome `<harness>:<arquivo>:<provedor>.<chave>` (ou `<harness>:<arquivo>:<chave>`, quando não há provedor em volta), ligado ao valor falso e, quando o nome da chave indica o provedor (como `OPENROUTER_API_KEY`), ao domínio dele. Depois disso, o harness usa o arquivo importado como está, e o gateway troca o falso dele pelo real. No hermes, que lê o `.env` por cima do ambiente, é assim que a chave importada passa a valer.

Cada segredo fica ligado aos domínios para onde ele pode ir: o gateway só troca o falso pelo real num pedido a um deles. Assim, um agente enganado que manda o falso para outro site manda só o falso. O comando descobre o domínio sozinho, nesta ordem:

1. **Nome da chave:** `OPENROUTER_API_KEY` vira `openrouter.ai`.
2. **Provedor em volta da linha:** o `"openrouter": {` do `auth.json` do opencode vira `openrouter.ai`.
3. **`baseURL` do provedor na configuração do opencode:** `http://127.0.0.1:8888/v1` vira `host.sandmux.internal` (o nome da sua máquina dentro do sandbox), e `https://llm.empresa.com/v1` vira `llm.empresa.com`.

Só quando nada disso indica o dono, o comando pergunta, depois do valor. A resposta pode ser `localhost`, `127.0.0.1` ou `::1` para um servidor na sua máquina, que viram `host.sandmux.internal`. O mesmo vale para `sandmux vault add --domain localhost`. Ao guardar, ele diz o domínio, ou avisa que o segredo ficou sem domínio. Uma resposta vazia guarda o segredo sem domínio, e então não há troca. Para um servidor local de modelo alcançado pelas [portas do host](network.md#portas-do-host) (LM Studio, Unsloth Studio, Ollama com chave), o domínio é `host.sandmux.internal`.

## `--pass-env` e a restauração

Com o cofre desbloqueado, os valores de `--pass-env` de uma sessão vão cifrados para o cofre. Depois de um reboot (ou de `sandmux daemon restart`), a sessão aparece como **aguardando cofre** em `sandmux ls` e na barra da TUI (`(cofre)` ao lado do nome), e sobe sozinha, com os mesmos valores, quando o cofre é desbloqueado. `sandmux kill` tira os valores da sessão do cofre.

Com o cofre ausente ou trancado no `run`, nada é guardado: o `run` avisa, e num reboot a sessão fica parada até um `sandmux restart` num terminal com as variáveis, como antes do cofre.

## Como é guardado

- **Arquivo:** `~/.local/share/sandmux/vault.json` (no diretório de dados), modo 600, gravado de forma atômica.
- **Cifra:** a chave vem da senha pelo Argon2id (64 MiB, 3 passadas), com um sal aleatório; o conteúdo é cifrado com XChaCha20-Poly1305, com um nonce novo a cada gravação e o cabeçalho como dado associado. Nada do conteúdo fica em claro: nem os valores, nem os nomes dos segredos, nem as sessões.
- **Memória:** enquanto desbloqueado, o daemon guarda a chave derivada e o conteúdo, e os apaga ao trancar. O processo do daemon fica marcado como não despejável, para que nem o `/proc/<pid>/mem` nem um core dump entreguem os segredos.
- **Senha esquecida:** não há como recuperar. Apague o arquivo e crie outro cofre.

## Limites

- A troca é só nos cabeçalhos dos pedidos HTTP/1.1 decifrados, inclusive dentro do `Authorization: Basic`. Provedores que pedem a chave no corpo, ou programas que fixam o certificado do servidor, não são atendidos.
- O `--pass-env` de uma variável sem segredo no cofre continua pondo o valor real dentro do sandbox.
- Um processo do mesmo usuário, fora do sandbox, com privilégio para ler a memória de outros processos (como um depurador com `CAP_SYS_PTRACE`), alcança o cofre aberto. A ameaça alvo do sandmux é o agente dentro do sandbox.
- O keyring do sistema fica para depois da v1.
- OAuth, renovação de tokens e registries que pedem certificado de cliente não são atendidos.
- Enquanto a sessão vive e o cofre está aberto, o agente assina quantas vezes quiser com as chaves escolhidas; cada assinatura fica no registro.
- As chaves SSH não chegam aos contêineres do Docker privado.
