# Configuração e perfis

O sandmux funciona sem nenhuma configuração: a baseline embutida e o preset do harness já são o padrão seguro. O arquivo de configuração guarda só as exceções que você quer repetir sem digitar a cada `sandmux run`.

## O arquivo

`$XDG_CONFIG_HOME/sandmux/config.toml`, ou `~/.config/sandmux/config.toml` quando `XDG_CONFIG_HOME` não está definido. O `sandmux doctor` mostra o caminho e se o arquivo é válido.

O arquivo é opcional. Ele tem três tipos de seção de política, e mais duas que não mudam os sandboxes: `[tui]`, com a tecla de prefixo, os atalhos, o tema e a barra da interface ([docs/tui.md](tui.md)), e `[assistant]`, com os modelos do chat e do modal de comando e os provedores próprios ([docs/chat.md](chat.md)):

```toml
# Vale para toda execução.
[global]
ro = ["~/docs"]

# Perfil nomeado: só entra com `sandmux run --profile vm`.
[profile.vm]
kvm = true

# Perfil de projeto: entra sozinho na pasta e nas subpastas.
[project."/home/ana/proj"]
gpu = false
clipboard = true
pass-env = ["OPENROUTER_API_KEY"]
```

| Chave | Tipo | Mesmo efeito de |
|---|---|---|
| `ro` | lista de caminhos | `--ro` |
| `rw` | lista de caminhos | `--rw` |
| `pass-env` | lista de nomes de variáveis | `--pass-env` |
| `repo` | true ou false | `--repo` / `--no-repo` |
| `kvm` | true ou false | `--kvm` / `--no-kvm` |
| `gpu` | true ou false | `--gpu` / `--no-gpu` |
| `clipboard` | true ou false | `--allow-clipboard` / `--no-allow-clipboard` |
| `harness-asks` | true ou false | (só na configuração; veja abaixo) |
| `window` | true, false ou `"direct"` | `--window` / `--window-direct` / `--no-window` (veja [window.md](window.md)) |
| `memory` | texto: `"50%"`, `"16G"` ou `"max"` | `--memory` (veja [limits.md](limits.md)) |
| `processes` | número ou `"max"` | `--processes` |
| `network` | `"aberta"`, `"inferencia"`, `"custom"` ou `"sem-rede"` | `--network` (veja [network.md](network.md)) |
| `deny` | lista de padrões de destino | `--deny` |
| `allow` | lista de padrões de destino (modo `custom`) | `--allow` |
| `allow-off` | lista de padrões cadastrados e desligados | (sem opção) |
| `catalog` | lista de grupos do catálogo de domínios comuns | (sem opção) |
| `catalog-off` | lista de itens desligados dos catálogos de exfiltração e de inferência | (sem opção) |
| `decrypt` | lista de padrões de destino cujo TLS é decifrado ([network.md](network.md)) | `--decrypt` |
| `file-deny` | lista de padrões de arquivo que não abrem ([files.md](files.md#política-de-arquivos)) | `--file-deny` |
| `file-allow` | lista de padrões de arquivo liberados, que vencem `file-deny` e os segredos | `--file-allow` |
| `secret-files` | lista de padrões somados à lista de arquivos com cara de segredo | (sem opção) |
| `secret-files-off` | lista de padrões que deixam de contar como segredo | (sem opção) |
| `host-ports` | lista de números de porta do host levadas ao `127.0.0.1` do sandbox, para serviços locais como o LM Studio ([network.md](network.md#portas-do-host)) | `--host-port` |
| `ports` | lista de portas de entrada publicadas, `8642` (só no `127.0.0.1`) ou `8443@0.0.0.0` (também no endereço); quando não está vazia, só elas saem do sandbox ([network.md](network.md#allowlist-de-portas)) | `--port` |
| `db` | lista de nomes de conexões de banco do cofre que o MCP de banco da sessão pode usar ([database.md](database.md)); `"*"` libera todas | `--db` |
| `mcp-deny` | lista de nomes de servidores MCP desabilitados ([mcp.md](mcp.md)) | `--mcp-deny` |
| `mcp-allow` | lista de nomes de servidores MCP; quando não está vazia, só eles ficam habilitados | `--mcp-allow` |
| `ssh-keys` | lista de nomes de chaves SSH do [cofre](vault.md#chaves-ssh) que o agente SSH da sessão pode usar; `"*"` libera todas | `--ssh-key` |
| `net-log-days` | número de dias, só em `[global]` | (retenção do registro de rede, padrão 30) |
| `net-log-max` | tamanho, como `"500M"`, só em `[global]` | (teto do registro de rede, padrão `"500M"`) |
| `notify` | `true` ou `false`, só em `[global]` | (notificações do desktop: bloqueios, chaves reais, abas encerradas; padrão `true`; [tui.md](tui.md#notificações-do-desktop)) |
| `screen-history` | `true` ou `false`, só em `[global]` | (histórico de tela cifrado em disco, com a chave no cofre; padrão `false`; [sessions.md](sessions.md#histórico-em-disco)) |
| `language` | `"pt"` ou `"en"`, só em `[global]` | (idioma do sandmux: TUI, linha de comando e mensagens; sem a chave, o do sistema; [veja abaixo](#idioma)) |

- **Caminhos:** absolutos ou começando por `~/`.
- **`pass-env`:** guarda só nomes, nunca valores. O valor vem do ambiente de quem lança, com o mesmo aviso da opção. Uma variável ausente recusa o lançamento.
- **Nomes de perfil:** letras minúsculas, números, `-` e `_`.
- **Pastas de projeto:** caminhos absolutos.

Um arquivo inválido faz o `run` recusar o lançamento, com o arquivo, a linha e o motivo: TOML malformado, seção ou chave desconhecida, tipo errado, caminho relativo, valor em `pass-env`, ou o mesmo caminho em `ro` e `rw` da mesma seção. Nada do arquivo é ignorado em silêncio.


### Perguntas do harness

Por padrão, o sandmux lança os harnesses no modo em que eles não pedem confirmação para editar, rodar comandos, sair da pasta do projeto ou ler arquivos como o `.env`, porque quem bloqueia é o sandbox (Landlock, a [política de arquivos](files.md) e a [rede](network.md)):

| Harness | Como |
|---|---|
| opencode | `OPENCODE_PERMISSION` com tudo em `allow` |
| Claude Code | `--dangerously-skip-permissions` e `--settings '{"skipDangerousModePermissionPrompt":true}'` |
| hermes | `HERMES_YOLO_MODE=1` (os bloqueios fixos do hermes continuam) |
| oh-my-pi | `--auto-approve` |

`harness-asks = true` (global, perfil ou projeto, ou o item "Perguntas do harness" na tela de configurações da TUI) lança o harness como ele é, com as perguntas. O ajuste vai pelo ambiente e pelos argumentos de cada lançamento: os arquivos de configuração do harness no HOME sintético não mudam. O `shell` e a aba de chat não são afetados.

## Camadas

A política de cada lançamento soma camadas, da menos para a mais específica:

1. baseline embutida;
2. preset do harness;
3. `[global]`;
4. perfil nomeado (`--profile`);
5. perfil do projeto;
6. opções do `run`.

- **Liga e desliga, tetos e rede** (`repo`, `kvm`, `gpu`, `clipboard`, `memory`, `processes`, `network`): vence a camada mais específica que definiu a chave. Um `gpu = false` no global é desfeito por `gpu = true` no projeto, e o projeto é desfeito por `--no-gpu` no `run`.
- **Listas** (`ro`, `rw`, `pass-env`, `deny`, `allow`, `allow-off`, `catalog`, `catalog-off`, `decrypt`, `ssh-keys`, `file-deny`, `file-allow`, `secret-files`, `secret-files-off`, `mcp-deny`, `mcp-allow`, `db`, `ports`): os itens de todas as camadas se somam; numa porta que aparece em duas camadas, vale o endereço da mais específica. Na rede, negar sempre vence ([network.md](network.md)). Se o mesmo caminho está em `ro` numa camada e em `rw` noutra, vale a mais específica.
- **Baseline fixa:** os perfis só acrescentam ou ajustam essas chaves. Os itens da baseline (arquivos de `/etc` cobertos, toolchains, caches, credenciais conhecidas) não podem ser desligados por perfil.

### Perfil do projeto

Entra quando a pasta de trabalho, com os links resolvidos, é a pasta da seção ou uma subpasta dela. Com perfis em pastas aninhadas, vale só o da pasta mais próxima. `/home/ana/proj` não vale para `/home/ana/proj2`.

O perfil de projeto fica na sua configuração, fora da pasta. Nada dentro da pasta de trabalho muda a política, porque o agente pode escrever ali: um `.sandmux.toml` ou um `.config/sandmux/config.toml` na pasta são ignorados. Custo aceito: o perfil não vai junto com o repositório.

## Comandos

| Comando | Efeito |
|---|---|
| `sandmux config show [--workdir PASTA] [--profile NOME]` | A política efetiva para a pasta atual. Cada chave aparece como "padrão" ou "modificada", com a camada de onde veio |
| `sandmux config list` | Os perfis nomeados e as pastas com perfil de projeto, com as chaves de cada um. Uma pasta que não existe mais aparece marcada |
| `sandmux config set <chave> <valor>` | Define uma chave de liga e desliga (`true` ou `false`, também `sim` e `não`) ou um teto (`memory`, `processes`), ou o modo de rede (`network`) |
| `sandmux config unset <chave>` | Apaga uma chave de liga e desliga ou um teto |
| `sandmux config add <chave> <item>` | Acrescenta um item a `ro`, `rw`, `pass-env`, `deny`, `allow`, `allow-off`, `catalog`, `catalog-off`, `decrypt`, `ssh-keys`, `file-deny`, `file-allow`, `secret-files`, `secret-files-off`, `mcp-deny`, `mcp-allow`, `db` ou `ports` |
| `sandmux config remove <chave> <item>` | Retira um item de uma lista |

`set`, `unset`, `add` e `remove` exigem o alvo:

- `--global`;
- `--profile <nome>`;
- `--project [<pasta>]`, que sem pasta usa a atual.

O comando:

- valida o resultado antes de gravar;
- grava só o que você definiu, nunca os valores padrão;
- preserva comentários e a formatação do que você escreveu à mão;
- apaga a seção que ficou vazia;
- grava de forma atômica.

Um arquivo novo é criado com modo 600 e a pasta com 700.

A [página de configurações da TUI](tui.md#configurações) (Ctrl+b `e`) grava pelo mesmo editor, com as mesmas regras, e também as chaves de `[tui]`, `[assistant]` e as que só valem em `[global]` (`net-log-days`, `net-log-max`, `language`, `notify`, `screen-history`).

## Idioma

O sandmux fala português ou inglês: a TUI, a saída e os erros da linha de comando, os avisos do daemon e as mensagens de validação deste arquivo. O idioma vem, nesta ordem:

1. da variável `SANDMUX_LANG` (`pt` ou `en`);
2. da chave `language` de `[global]`;
3. do sistema: `LC_ALL`, `LC_MESSAGES` ou `LANG`, o primeiro que estiver definido. Um valor começado por `pt` é português; qualquer outro, inclusive `C`, é inglês.

```toml
[global]
language = "en"
```

Os nomes das chaves, seções e valores deste arquivo, dos subcomandos e das opções são os mesmos nos dois idiomas; em inglês, a TUI e o `config show` mostram o nome traduzido de alguns valores (`aberta` aparece como `open (aberta)`), e o arquivo continua com o valor em português. O cliente diz o idioma dele na saudação, e o daemon responde nesse idioma. A página de configurações da TUI tem o idioma na seção Geral, e a troca vale na hora.

Exemplos:

```bash
sandmux config set kvm true --profile vm
```

```bash
sandmux config add ro ~/docs --project
```

```bash
sandmux config show
```

## A configuração fora do alcance do agente

O sandbox nunca recebe a pasta de configuração com escrita. São recusados como pasta de trabalho, em `--rw` ou na chave `rw`:

- a própria pasta;
- uma pasta dentro dela;
- uma pasta que a contém, como `--rw ~/.config`.

Em somente leitura ela pode entrar (`--ro ~/.config`), porque não guarda segredo.

## Caminhos que o host executa

Também são recusados com escrita, em `--rw`, na chave `rw`, como pasta de trabalho e nas pastas montadas ao vivo, os caminhos do HOME que o host executa fora do sandbox. Com escrita neles, o agente poderia plantar algo que roda no próximo terminal ou login:

- arquivos de inicialização de shell e de sessão: `.bashrc`, `.bash_profile`, `.bash_login`, `.bash_logout`, `.profile`, `.zshrc`, `.zprofile`, `.zshenv`, `.zlogin`, `.zlogout`, `.xprofile`, `.xinitrc`, `.xsession`, `.xsessionrc` e `.pam_environment`;
- pastas: `.bashrc.d`, `.oh-my-zsh`, `.local/bin`, `.local/share/applications`, `.config/autostart`, `.config/systemd`, `.config/environment.d`, `.config/fish` e `.config/plasma-workspace`.

Vale para o caminho, para o que está dentro dele e para uma pasta que o contém. Em somente leitura, todos podem entrar (`--ro ~/.local/bin`).

## Quando uma mudança vale

A política é resolvida a cada lançamento, com o arquivo daquele momento. A lista de sessões guarda as opções dadas no `run` e o nome do perfil, e não a política resolvida.

- **Sessão viva:** continua com a política do lançamento. A mudança vale quando ela for relançada (`sandmux restart`, `sandmux daemon restart` ou reboot).
- **Restauração depois do reboot:** uma sessão viva fica parada, com o motivo em `sandmux ls`, quando:
  - a política dela passou a ter `pass-env`;
  - o perfil nomeado dela foi apagado;
  - o arquivo está inválido.
- **`sandmux restart` de uma sessão cujo perfil foi apagado:** termina com erro, e a sessão continua como estava.

## Cliente e daemon

O daemon usa o mesmo arquivo de configuração do cliente. A saudação confere a pasta de configuração, e um cliente com outro `XDG_CONFIG_HOME` é recusado, com as duas pastas na mensagem. A unidade systemd leva o `XDG_CONFIG_HOME` do cliente que a instalou.

## Voltar para a versão anterior

A versão anterior do sandmux ignora o arquivo de configuração. O daemon dela não lê a lista de sessões no formato novo: depois de instalar o binário anterior e rodar `sandmux daemon restart`, a lista de sessões começa vazia. Os HOMEs sintéticos e as camadas de escrita não são afetados.
