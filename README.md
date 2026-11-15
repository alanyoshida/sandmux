# sandmux

> English: [README.en.md](README.en.md)

Multiplexador de terminal para rodar agentes de código dentro de um sandbox de segurança.

O sandmux sobe harnesses de agente (opencode, hermes) em abas, no estilo do tmux. Cada aba roda confinada: o agente enxerga a pasta de trabalho e mais nada do seu HOME, o tráfego de rede é registrado e pode ser bloqueado, e nenhuma credencial real entra no sandbox.

> **Situação: em desenvolvimento, antes da v1.** Existem e têm testes: o confinamento, o daemon de sessões, a TUI com abas e painel, a configuração com perfis, os tetos de recurso, o Docker privado por pasta, a rede pelo gateway com registro e filtro de domínios, as portas publicadas, o registro de acesso a arquivos, o cofre de segredos com a troca de chaves e tokens no tráfego e o agente SSH do git. As etapas 7 (MCP e banco de dados) e 8 (hermes completo: portas de entrada, gateway, canais e cron) estão completas; a etapa 9 (assistente: aba de chat e modal de comando) também. A etapa 10 traz o instalador, os pacotes e a primeira release. A seção "O que já funciona" descreve o que existe; o resto do documento descreve o que foi decidido para a v1.

## Por quê

Um harness de agente roda hoje com o mesmo acesso que você: lê `~/.ssh`, credenciais de cloud, tokens de outros programas e qualquer pasta do disco. Um agente manipulado por prompt injection pode ler esses segredos e enviá-los para fora.

O sandmux parte de um princípio: **seguro por padrão, com o mínimo de atrito**. Uma baseline segura vem embutida e funciona sem configurar nada. Você só gerencia as exceções.

## Como funciona

Você entra na pasta do projeto e sobe o agente:

```bash
cd ~/projetos/app
sandmux run opencode
```

A pasta de onde o comando foi executado é montada dentro do sandbox no mesmo caminho, com leitura e escrita. É a pasta real, não uma cópia: o código que o agente cria aparece no host na hora, e você pode abri-lo no editor enquanto ele trabalha.

```
host                                  sandbox
~/projetos/app/   <-- mesma pasta --> ~/projetos/app/    leitura e escrita
~/ (resto)                            ~/ (HOME sintetico, separado)
/usr, /etc, ...   ------------------> somente leitura
```

O que o agente enxerga:

| Recurso | Dentro do sandbox |
|---|---|
| Pasta de trabalho | A pasta real, com leitura e escrita. `.git/hooks` e `.git/config` ficam em somente leitura |
| HOME | Um HOME sintético por harness. O HOME real não existe lá dentro |
| Sistema (`/usr`, `/etc`) | Somente leitura, com os arquivos sensíveis cobertos |
| Toolchains (cargo, nvm, uv, go e outras) | Lidas do host, escritas numa camada privada. `cargo install` e `npm install -g` funcionam sem alterar o host |
| Sockets do host (ssh-agent, D-Bus, Docker, Wayland, X11) | Fora do alcance |
| Processos do host | Invisíveis |
| Variáveis de ambiente | Só uma lista segura; o resto não é herdado |

Rodar o comando direto no HOME, na raiz ou numa pasta que contém o HOME é recusado. Tem que ser uma subpasta.

## O que já funciona

### Instalação

```bash
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador. Sem `sudo` (recusado ou com `--no-sudo`), instala em `~/.local/bin`. A release também tem `.deb`, `.rpm`, o tarball, o `PKGBUILD` do AUR e a fórmula do Homebrew. Pelo código:

```bash
cargo install --git https://github.com/alanyoshida/sandmux sandmux
```

Depois de instalar por pacote ou por `cargo install`, rode:

```bash
sandmux setup
```

O `setup` instala o perfil do AppArmor do lançador quando a distro restringe user namespaces (o Ubuntu, por padrão), pedindo `sudo`. Ele também habilita o daemon e termina com o `sandmux doctor`, que confere o kernel (Landlock ABI 6 ou mais novo), os user namespaces, a camada de escrita, os tetos de recurso e a rede. Detalhes em [docs/install.md](docs/install.md).

### Comandos

| Comando | O que faz |
|---|---|
| `sandmux` (ou `sandmux tui`) | Abre a interface com abas, uma por sessão, com a barra em blocos coloridos embaixo: troca com Ctrl+b n/p/1–9, nova aba com Ctrl+b c, lista com Ctrl+b w, menu com Ctrl+b `:`, configurações com Ctrl+b `e` (toggles e temas como Catppuccin, Rosé Pine, Tokyo Night, Kanagawa, Gruvbox e Dracula, gravados no `config.toml`), sair com Ctrl+b d ([docs/tui.md](docs/tui.md)) |
| `sandmux run <harness>` | Sobe o harness confinado na pasta atual. Harnesses: `opencode`, `hermes`, `claude` (o Claude Code, com o login da assinatura fora do sandbox; [docs/claude.md](docs/claude.md)) `omp` (o oh-my-pi; [docs/omp.md](docs/omp.md)) e `shell` (um shell confinado, sem agente). Na primeira execução, pergunta se deve importar a configuração que você já tem. Num terminal, cria uma sessão no daemon e anexa a ela; sem terminal (script, pipe, CI), roda direto, em primeiro plano, sem daemon |
| `sandmux ls` | Lista as sessões: identificador, nome, grupo, estado (viva, encerrada com o código, parada com o motivo), terminais anexados e pasta |
| `sandmux attach [id]` | Anexa este terminal a uma sessão. Sem identificador, à única sessão viva |
| `sandmux group list\|move\|rename` | Grupos de abas, como as sessões do tmux: cada TUI mostra um grupo, e `sandmux run --group <grupo>` cria a sessão nele ([docs/sessions.md](docs/sessions.md#grupos-de-abas)) |
| `sandmux enter [id]` | Abre um shell dentro do sandbox de uma sessão viva, com o que o agente vê (Ctrl+b `t` na TUI; [docs/sessions.md](docs/sessions.md#shell-dentro-do-sandbox-de-uma-aba)) |
| `sandmux detach <id>` | Desanexa todos os terminais de uma sessão, sem encerrá-la |
| `sandmux kill <id>` | Encerra a sessão e a tira da lista |
| `sandmux rename <id> <nome>` | Troca o nome de uma sessão (o nome da aba) |
| `sandmux stats <id>` | Mostra a memória e os processos da sessão: em uso, picos, tetos, CPU, e o que o teto barrou |
| `sandmux restart <id>` | Relança uma sessão encerrada ou parada, na mesma pasta e com as mesmas opções |
| `sandmux daemon status`, `stop`, `restart`, `disable`, `boot` | Estado do daemon; parar (as sessões vivas voltam na próxima subida); reiniciar com o binário instalado; parar e remover a unidade systemd; subir no boot, sem login, com as abas voltando no primeiro login ([docs/sessions.md](docs/sessions.md#início-no-boot)) |
| `sandmux config show`, `list`, `set`, `unset`, `add`, `remove` | Mostra a política efetiva com a origem de cada valor, lista os perfis e altera o arquivo de configuração (`~/.config/sandmux/config.toml`) |
| `sandmux import <harness>` | Copia a configuração existente do harness para o HOME sintético, trocando os segredos por valores falsos. Rodado de novo, copia só o que é novo; `--overwrite` substitui. No `shell`, traz o `.bashrc`, o `.zshrc` e afins do shell em uso, com o framework ([docs/sessions.md](docs/sessions.md#configuração-do-shell)) |
| `sandmux update <harness>` | Roda o atualizador oficial do harness no host, fora do sandbox, e diz quais abas reiniciar ([docs/sessions.md](docs/sessions.md#atualizar-o-harness)) |
| `sandmux doctor --fix` | Leva para o cofre as chaves reais gravadas dentro do sandbox (o "Connect provider" do opencode, por exemplo), deixando valores falsos no lugar |
| `sandmux doctor` | Mostra se a máquina tem o suporte necessário e o que falta |

Opções de `run`:

| Opção | Efeito |
|---|---|
| `--workdir <pasta>` | Usa outra pasta de trabalho no lugar da pasta atual |
| `--ro <caminho>`, `--rw <caminho>` | Acrescenta um caminho do host em somente leitura ou com escrita. Montar uma credencial conhecida (`~/.kube`, `~/.aws`) gera aviso; escrita em caminhos que o host executa (`~/.bashrc`, `~/.local/bin`) é recusada ([docs/config.md](docs/config.md#caminhos-que-o-host-executa)) |
| `--pass-env <NOME>` | Passa uma variável do seu ambiente para dentro, com aviso. Para chaves de API e tokens, prefira o cofre, que entrega só um valor falso |
| `--ssh-key <NOME>` | Deixa o agente SSH da sessão usar a chave SSH do cofre com esse nome (`*` para todas) |
| `--import`, `--no-import` | Responde à pergunta da primeira execução sem terminal interativo |
| `--profile <nome>` | Aplica um perfil nomeado do arquivo de configuração |
| `--repo`, `--no-repo` | Numa subpasta de repositório, monta (ou não) o repositório inteiro em somente leitura para o git funcionar |
| `--kvm`, `--no-kvm` | Libera (ou não) `/dev/kvm` |
| `--gpu`, `--no-gpu` | Expõe (ou não) a GPU, exposta por padrão quando existe |
| `--memory <teto>`, `--processes <teto>` | Muda o teto de memória (`50%`, `16G`, `max`) ou de processos (`512`, `max`) |
| `--network <modo>` | `aberta` (padrão), `inferencia`, `custom` ou `sem-rede` |
| `--deny <padrão>`, `--allow <padrão>` | Nega ou libera (no modo `custom`) um destino nesta aba: `exemplo.com`, `*.exemplo.com`, IPv4 ou CIDR |
| `--allow-clipboard`, `--no-allow-clipboard` | Deixa (ou não) o harness gravar no clipboard do seu terminal (OSC 52). Por padrão a gravação é bloqueada, e a leitura do clipboard é sempre bloqueada |
| `-- <argumentos>` | Argumentos repassados ao harness |

Exemplos:

```bash
sandmux run shell -- -c "id -u; ls ~"
```

```bash
sandmux run opencode --pass-env OPENROUTER_API_KEY
```

As opções negativas existem para desfazer o que um perfil ligou.

### Configuração e perfis

O arquivo `~/.config/sandmux/config.toml` guarda as exceções que você quer repetir: ajustes globais (`[global]`), perfis nomeados (`[profile.<nome>]`, com `--profile`) e perfis de projeto (`[project."<pasta>"]`), que entram sozinhos na pasta e nas subpastas. As chaves são as mesmas das opções do `run`: `ro`, `rw`, `pass-env` (só nomes), `repo`, `kvm`, `gpu`, `clipboard`, `memory`, `processes` e `network`. Vence a camada mais específica, e as opções do `run` vencem todas. O agente nunca consegue gravar nesse arquivo, e nada dentro da pasta de trabalho muda a política. Detalhes em [docs/config.md](docs/config.md).

```bash
sandmux config set kvm true --profile vm
```

### Docker privado

Cada pasta de trabalho tem o próprio Docker sem root, que o agente usa de dentro da sessão com `docker` e `docker compose`. Os contêineres só enxergam o que o sandbox enxerga: a pasta de trabalho e o sistema, nunca o HOME. As portas publicadas ficam só no `127.0.0.1` do host, e de dentro do sandbox são alcançadas por `host.sandmux.internal`. O tráfego dos contêineres passa pelo mesmo gateway do sandbox, com o registro e a política mais restrita das abas da pasta. Com o pacote `docker-registry` instalado, as imagens do Docker Hub ficam num cache comum entre as pastas. O Docker sobe no primeiro `docker` e para com a última sessão da pasta. Imagens e volumes persistem por pasta. O socket do Docker do host nunca entra. Detalhes em [docs/docker.md](docs/docker.md).

### Rede

Cada sandbox tem rede própria, e todo o tráfego passa por um gateway que refaz cada conexão a partir do host e registra o destino: domínio, porta, bytes e duração, nunca o conteúdo. Os metadados de cloud, o QUIC e o ICMP são bloqueados. Os serviços da sua máquina ficam em `host.sandmux.internal`. Para um harness configurado com um serviço local (o LM Studio em `127.0.0.1:8888`, um MCP local), `host-ports` leva a porta do host ao `127.0.0.1` do sandbox, sem editar a configuração; a chave desse servidor fica no cofre, e no opencode os modelos do servidor local aparecem sozinhos.

| Modo (`--network`) | O que passa |
|---|---|
| `aberta` (padrão) | tudo, menos a denylist, que já vem com um catálogo de destinos de exfiltração (colagem, envio de arquivos, túneis) |
| `inferencia` | só as APIs de inferência (OpenRouter, Anthropic, OpenAI e outras) |
| `custom` | só a allowlist, montada com `allow` e grupos de domínios comuns (github, npm, pypi…) |
| `sem-rede` | nada |

Uma regra nova vale na conexão seguinte da sessão, sem relançar o agente. Nos domínios donos dos segredos do cofre (e nos da chave `decrypt`), o gateway decifra o TLS com uma CA própria de cada lançamento e registra o método e o caminho de cada pedido. Um servidor que o agente sobe (como `npm run dev` na porta 3000) é publicado sozinho no `127.0.0.1` do host, e o painel mostra a porta. A chave `ports` troca isso por uma allowlist, e uma porta como `8443@0.0.0.0` recebe webhooks de fora, com cada conexão no registro. O `sandmux net` mostra o registro e muda as regras da aba; na TUI, o painel mostra as últimas conexões e Ctrl+b `a` abre a tela de rede, que nega e libera direto dos acessos. Detalhes em [docs/network.md](docs/network.md).

```bash
sandmux net 1
```

### Arquivos

Toda abertura de arquivo do agente passa pelo supervisor do sandbox (seccomp com notificação): ele abre o arquivo dentro da raiz do sandbox, confere as regras do Landlock e entrega o descritor, sem a corrida de trocar o caminho depois da checagem. Os acessos às pastas do agente (pasta de trabalho, HOME sintético e pastas extras) vão para um registro, e `sandmux files` mostra os de uma sessão. Arquivos com cara de segredo da pasta de trabalho (`.env`, chaves privadas, `.npmrc` e afins) não abrem para o agente, a não ser os que ele mesmo criar; `file-deny`, `sandmux files deny` e a tela Ctrl+b `f` negam arquivos na hora; e um `.git/commondir` criado pelo agente é negado. Uma pasta nova entra na sessão viva em `/mnt/sandmux/<nome>` (`sandmux folder add`, ou Ctrl+b `o`), sem relançar o agente. O custo medido foi de 1,0x a 1,3x no uso normal. Detalhes em [docs/files.md](docs/files.md).

### MCPs

O sandmux lista os servidores MCP de cada harness (os importados ficam habilitados) e habilita ou desabilita cada um inteiro: `mcp-deny` e `mcp-allow` na configuração, `sandmux mcp deny|allow|forget` na aba, ou Ctrl+b `m` na TUI. No opencode, os desabilitados vão no `OPENCODE_CONFIG_CONTENT`; no hermes, num `config.yaml` só daquela execução. Detalhes em [docs/mcp.md](docs/mcp.md).

### Banco de dados

O MCP de banco embutido consulta e altera bancos PostgreSQL e MySQL cadastrados no cofre (`sandmux db add`), sem a senha entrar no sandbox: a sessão escolhe as conexões com `--db`, o harness recebe o MCP `sandmux-db`, e cada query vai para um registro (`sandmux db log`). Detalhes em [docs/database.md](docs/database.md).

### Hermes: canais e cron

`sandmux run hermes --gateway` roda o gateway do hermes, com os canais de mensagem (Telegram, Discord e Slack) e as tarefas agendadas, numa sessão própria do daemon. A sessão reinicia se o gateway cair e, depois de um reboot, espera o cofre ser desbloqueado. Os tokens dos canais ficam no cofre (`sandmux vault add telegram`), e o agente só vê um falso: o gateway do sandmux troca o falso pelo real na URL do Telegram, no WebSocket do Discord e nos cabeçalhos do Slack. Detalhes em [docs/hermes.md](docs/hermes.md).

### Chat com os modelos

Ctrl+b `i` na TUI, ou `sandmux chat "pergunta"`, abre uma aba de chat com um modelo cuja chave está no cofre, escrito `<provedor>/<modelo>` (o padrão vem de `chat-model` em `[assistant]`). A aba roda confinada, só com rede de inferência, e a chave real nunca entra: o gateway troca o falso no tráfego. A conversa fica na pasta privada da aba, volta depois de um reinício e é apagada quando a aba fecha. Sem terminal, `sandmux chat` imprime só a resposta. Ctrl+b `g` abre o modal de comando: o modelo recebe só a pergunta, a pasta da aba e o shell, e o comando sugerido é escrito na linha da aba sem executar. Detalhes em [docs/chat.md](docs/chat.md).

### Cofre de segredos

`sandmux vault init` cria um cofre cifrado com senha (Argon2id e XChaCha20-Poly1305) para chaves de API e tokens, fora do sandbox. O daemon sobe com ele trancado; a TUI pede a senha ao abrir, ou `sandmux vault unlock` no terminal. Opcionalmente, `sandmux vault keyring on` guarda a senha no keyring do sistema, e o daemon abre o cofre sozinho. Os valores de `--pass-env` vão para o cofre, e uma sessão que os usa volta sozinha depois de um reboot, assim que o cofre é desbloqueado. `sandmux vault import` pede os valores reais das chaves que a importação trocou por falsas. Detalhes em [docs/vault.md](docs/vault.md).

Com a chave do provedor no cofre (`sandmux vault add openrouter --domain openrouter.ai`), uma sessão do opencode ou do hermes recebe só um valor falso; o gateway troca o falso pelo real no pedido ao `openrouter.ai`, e a chave nunca entra no sandbox. O `--pass-env` deixa de ser necessário para as chaves de inferência.

O mesmo vale para as ferramentas de desenvolvimento: com um token do GitHub no cofre (`sandmux vault add github --domain github.com`), o `gh` e o `git push` por HTTPS usam um valor falso, e o gateway o troca pelo real, inclusive dentro do `Authorization: Basic`. Registries privados de npm, pip, cargo e Docker funcionam do mesmo jeito (`--tool npm|pip|cargo|docker`).

Para o git por SSH, a chave privada vai para o cofre (`sandmux vault add-ssh github --file ~/.ssh/id_ed25519 --server github.com`) e a sessão escolhe as chaves que pode usar (`--ssh-key github`, ou `ssh-keys` num perfil). O agente SSH do sandmux assina dentro do sandbox sem a chave entrar nele, cada assinatura fica no registro, e o SSH de saída só chega aos servidores das chaves escolhidas.

### Tetos de memória e de processos

Cada sandbox roda com um teto de 75% da RAM, sem swap, e de 8192 processos e threads. Quem estoura o teto é morto ou barrado só dentro da sessão. Os tetos vêm de um escopo do systemd de usuário; sem ele, a sessão roda sem teto, com aviso. O `sandmux ls` mostra o consumo de cada sessão. Detalhes em [docs/limits.md](docs/limits.md).

### Sessões

Num terminal, `sandmux run` cria uma sessão no daemon de sessões (`sandmuxd`) e anexa o terminal a ela. O agente continua rodando quando o terminal fecha, e `sandmux attach` volta a ele de qualquer terminal, com o histórico e a tela atual. Enquanto anexado:

| Teclas | Efeito |
|---|---|
| Ctrl+b d | Desanexa; a sessão continua |
| Ctrl+b Ctrl+b | Envia um Ctrl+b ao harness |

Vários terminais podem se anexar à mesma sessão, e todos digitam. A tela segue o menor deles. O histórico (10.000 linhas por sessão) fica só em memória e some quando o daemon para.

Na primeira vez em que precisa do daemon, o cliente instala e habilita a unidade systemd de usuário `sandmuxd.service`, que sobe o daemon no login, e avisa. Sem systemd, o cliente sobe o daemon na hora. Depois de um reboot, as sessões que estavam vivas voltam sozinhas. As que usavam `--pass-env` voltam quando o cofre é desbloqueado, se os valores foram guardados nele; sem cofre, ficam paradas até um `sandmux restart <id>` num terminal com as variáveis. Detalhes em [docs/sessions.md](docs/sessions.md).

Para entregar ao agente um arquivo de fora da pasta de trabalho (um PDF, um screenshot), copie-o para a pasta de entrada que o `doctor` mostra. Lá dentro ela aparece em `~/inbox`, em somente leitura.

Os dados do sandmux (HOMEs sintéticos, camadas de escrita, pasta de entrada, manifestos de importação) ficam em `~/.local/share/sandmux`. Para apagar uma camada de escrita, libere a permissão antes, porque o overlay cria pastas sem permissão: `chmod -R u+rwX ~/.local/share/sandmux/layers && rm -rf ~/.local/share/sandmux/layers/<harness>`.

### Limites desta etapa

- No modo aberta, o agente alcança qualquer destino fora da denylist. Para restringir, use os modos `custom` ou `inferencia`.
- Uma variável passada com `--pass-env` fica ao alcance do agente.
- No hermes, o `--pass-env` coloca o valor real no `~/.hermes/.env` só daquela execução, e o arquivo fica em somente leitura: o hermes não consegue salvar configurações no `.env` enquanto roda assim. O `.env` guardado continua com os valores falsos. Valores com quebra de linha ou `${` são recusados.
- O agente não instala pacotes do sistema (o caminho é um contêiner no Docker privado) e não abre programas com janela.
- A proteção do `.git` tem limites conhecidos, descritos em [docs/git.md](docs/git.md).

## O que está planejado para a v1

- **Confinamento**: user, mount, pid e network namespaces, Landlock e seccomp, sem precisar de root. Sem o suporte de kernel exigido, o agente não sobe; não existe aba sem sandbox.
- **Sessões persistentes**: um daemon mantém os agentes vivos quando o terminal fecha.
- **Interface**: abas com terminal confinado, painel lateral com o registro de acessos, tecla de prefixo (Ctrl+b por padrão), mouse e temas.
- **Rede**: todo o tráfego passa por um gateway que registra cada domínio. Quatro modos por aba: aberta (padrão), só inferência, sem rede e custom (bloqueado por padrão, com allowlist).
- **Segredos fora do sandbox**: um cofre cifrado guarda as chaves reais. O agente recebe uma chave falsa, trocada pela real no tráfego de saída, só quando o destino é o dono da chave. Vale para a API de inferência, para o `gh` e para registries privados.
- **Arquivos**: registro de todo acesso a arquivo, allowlist e denylist com efeito imediato, e arquivos com cara de segredo escondidos do agente.
- **Docker**: um Docker sem root, privado do sandbox. O socket do Docker do host nunca entra.
- **MCPs**: controle de quais servidores MCP estão liberados, e um MCP embutido para PostgreSQL e MySQL com a credencial injetada fora do sandbox.
- **Assistente**: um modal de chat e um modal que gera um comando shell e o escreve no terminal, sem executar.

A ordem de desenvolvimento, as decisões e os riscos aceitos estão no [roadmap](openspec/roadmap.md).

## Limites conhecidos

- **Ameaça alvo**: um agente manipulado que tenta ler segredos e exfiltrar dados. Exploit de kernel está fora do escopo.
- **A pasta de trabalho é do agente.** Ele lê tudo que está nela. O que ele escreve ali (`Makefile`, scripts do `package.json`, `.envrc`) precisa ser lido antes de ser executado no host.
- **No modo de rede padrão (aberta), o que não está na denylist passa**, registrado. Para restringir, use os modos `custom` ou `inferencia`.
- **Sem programas com janela na v1.** Navegador headless funciona.

## Requisitos

- Linux. Não há suporte a macOS nem a Windows.
- Kernel 6.12 ou mais novo (Landlock com escopo de sockets abstratos), com user namespaces sem privilégio e o `/dev/net/tun`. O overlayfs em user namespace é opcional: sem ele, os caches começam vazios e as toolchains ficam em somente leitura.
- Distros testadas e suportadas: Ubuntu 26.04, Debian 13, Fedora atual e Arch (as três últimas em contêineres, com o kernel da máquina de teste).
- Binários publicados só para `x86_64`; em outra arquitetura, compile com `cargo install`.
- Os harnesses (opencode, hermes, Claude Code, oh-my-pi) são instalados por você, no host. O sandmux não instala harness; `sandmux update <harness>` roda o atualizador oficial fora do sandbox.

## Arquitetura

Quatro binários, cada um com uma fronteira de confiança:

```
+----------------+  socket  +----------------------+  socket  +----------------+
| sandmux        |<-------->| sandmuxd             |<-------->| sandmux-helper |
| cliente        |          | daemon de sessoes    |          | auxiliar root  |
| (CLI e TUI)    |          | PTYs, cofre,         |          | eBPF           |
+----------------+          | registro, politica   |          | (pos-v1)       |
                            +----------+-----------+          +----------------+
                                       |
                                       | especificacao de lancamento
                                       v
                            +----------------------+
                            | sandmux-run          |
                            | lancador do sandbox  |
                            | gateway de rede      |
                            +----------+-----------+
                                       |
                                       v
                                    harness
```

- `sandmux`: cliente de linha de comando e interface de terminal. Não toca em segredo nem em sandbox.
- `sandmuxd`: daemon de sessões. Hospeda os terminais, o cofre, o registro e a política.
- `sandmux-run`: lançador do sandbox. Só aplica uma especificação pronta; não conhece harness nem política. O supervisor dele, fora do sandbox, roda o gateway de rede.
- `sandmux-helper`: auxiliar root para eBPF, previsto para depois da v1.

Escrito em Rust, com ratatui na interface.

## Desenvolvimento

O projeto usa [OpenSpec](https://github.com/Fission-AI/OpenSpec) para planejar cada mudança antes de implementar.

| Arquivo | Conteúdo |
|---|---|
| [docs/install.md](docs/install.md) | Instalação: `install.sh`, `sandmux setup`, pacotes, `cargo install`, a release e o teste nas distros |
| [docs/launch-spec.md](docs/launch-spec.md) | Formato da especificação de lançamento entre o cliente e o lançador |
| [docs/git.md](docs/git.md) | O que a proteção do `.git` cobre e como o git se comporta dentro do sandbox |
| [docs/sessions.md](docs/sessions.md) | O daemon de sessões: socket, protocolo, lista de sessões, restauração e limites |
| [docs/config.md](docs/config.md) | O arquivo de configuração: seções, chaves, camadas, perfis e comandos |
| [docs/tui.md](docs/tui.md) | Interface com abas: atalhos e `[tui]`, modais, aba encerrada, mouse, seleção e colagem de imagem |
| [docs/docker.md](docs/docker.md) | Docker privado por pasta: visão de arquivos, socket, ciclo de vida, persistência, portas e tetos |
| [docs/limits.md](docs/limits.md) | Tetos de memória e de processos: padrões, ajuste, escopo do systemd e medição |
| [docs/vault.md](docs/vault.md) | Cofre de segredos: criar, desbloquear, segredos, troca no tráfego, ferramentas (`gh`, `git`, `npm`, `pip`, `cargo`, `docker`), chaves SSH e o agente SSH, chaves da importação, `--pass-env` na restauração e como é guardado |
| [docs/database.md](docs/database.md) | MCP de banco: conexões no cofre, `--db`, as ferramentas, o registro das queries e os limites |
| [docs/chat.md](docs/chat.md) | Chat com os modelos: o modal Ctrl+b `i`, `sandmux chat`, a aba de chat, o modal de comando Ctrl+b `g`, modelos e provedores, `[assistant]`, o confinamento e a conversa guardada |
| [docs/hermes.md](docs/hermes.md) | Hermes: a sessão do gateway (`--gateway`), o reinício, a espera pelo cofre, os tokens dos canais e o webhook do Telegram |
| [docs/mcp.md](docs/mcp.md) | MCPs: inventário por harness, habilitar e desabilitar por servidor, `sandmux mcp` e a tela Ctrl+b `m` |
| [docs/files.md](docs/files.md) | Arquivos: aberturas pelo supervisor (seccomp), registro dos acessos às pastas do agente, `sandmux files` e retenção |
| [docs/network.md](docs/network.md) | Rede pelo gateway: endereços, `host.sandmux.internal`, modos, denylist, allowlist, catálogos, efeito imediato, registro e `sandmux net` |
| [openspec/roadmap.md](openspec/roadmap.md) | Todas as decisões do projeto e o plano em 10 etapas |
| [openspec/specs/](openspec/specs/) | Specs vigentes: isolamento, sistema de arquivos, presets dos harnesses, importação de estado, daemon e sessões, configuração, tetos de recurso, Docker privado e interface com abas |
| [openspec/changes/archive/2026-11-13-add-sandbox-cli/proposal.md](openspec/changes/archive/2026-11-13-add-sandbox-cli/proposal.md) | Proposta do primeiro change: o confinamento por linha de comando |
| [openspec/changes/archive/2026-11-13-add-sandbox-cli/design.md](openspec/changes/archive/2026-11-13-add-sandbox-cli/design.md) | Desenho técnico do primeiro change |
| [AGENTS.md](AGENTS.md) | Regras para agentes que trabalham neste repositório |

O primeiro change (`add-sandbox-cli`) está implementado e arquivado. Para rodar a verificação completa (formatação, lints e testes, inclusive os testes de fuga do sandbox): `./scripts/check.sh`.

## Licença

Licença dupla, MIT ou Apache-2.0, à sua escolha: [LICENSE-MIT](LICENSE-MIT) e [LICENSE-APACHE](LICENSE-APACHE).
