# Configuração e perfis

O sandmux funciona sem nenhuma configuração: a baseline embutida e o preset do harness já são o padrão seguro. O arquivo de configuração guarda só as exceções que você quer repetir sem digitar a cada `sandmux run`.

## O arquivo

`$XDG_CONFIG_HOME/sandmux/config.toml`, ou `~/.config/sandmux/config.toml` quando `XDG_CONFIG_HOME` não está definido. O `sandmux doctor` mostra o caminho e se o arquivo é válido.

O arquivo é opcional. Ele tem três tipos de seção:

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

- **Caminhos:** absolutos ou começando por `~/`.
- **`pass-env`:** guarda só nomes, nunca valores. O valor vem do ambiente de quem lança, com o mesmo aviso da opção. Uma variável ausente recusa o lançamento.
- **Nomes de perfil:** letras minúsculas, números, `-` e `_`.
- **Pastas de projeto:** caminhos absolutos.

Um arquivo inválido faz o `run` recusar o lançamento, com o arquivo, a linha e o motivo: TOML malformado, seção ou chave desconhecida, tipo errado, caminho relativo, valor em `pass-env`, ou o mesmo caminho em `ro` e `rw` da mesma seção. Nada do arquivo é ignorado em silêncio.

## Camadas

A política de cada lançamento soma camadas, da menos para a mais específica:

1. baseline embutida;
2. preset do harness;
3. `[global]`;
4. perfil nomeado (`--profile`);
5. perfil do projeto;
6. opções do `run`.

- **Liga e desliga** (`repo`, `kvm`, `gpu`, `clipboard`): vence a camada mais específica que definiu a chave. Um `gpu = false` no global é desfeito por `gpu = true` no projeto, e o projeto é desfeito por `--no-gpu` no `run`.
- **Listas** (`ro`, `rw`, `pass-env`): os itens de todas as camadas se somam. Se o mesmo caminho está em `ro` numa camada e em `rw` noutra, vale a mais específica.
- **Baseline fixa:** os perfis só acrescentam ou ajustam essas chaves. Os itens da baseline (arquivos de `/etc` cobertos, toolchains, caches, credenciais conhecidas) não podem ser desligados por perfil.

### Perfil do projeto

Entra quando a pasta de trabalho, com os links resolvidos, é a pasta da seção ou uma subpasta dela. Com perfis em pastas aninhadas, vale só o da pasta mais próxima. `/home/ana/proj` não vale para `/home/ana/proj2`.

O perfil de projeto fica na sua configuração, fora da pasta. Nada dentro da pasta de trabalho muda a política, porque o agente pode escrever ali: um `.sandmux.toml` ou um `.config/sandmux/config.toml` na pasta são ignorados. Custo aceito: o perfil não vai junto com o repositório.

## Comandos

| Comando | Efeito |
|---|---|
| `sandmux config show [--workdir PASTA] [--profile NOME]` | A política efetiva para a pasta atual. Cada chave aparece como "padrão" ou "modificada", com a camada de onde veio |
| `sandmux config list` | Os perfis nomeados e as pastas com perfil de projeto, com as chaves de cada um. Uma pasta que não existe mais aparece marcada |
| `sandmux config set <chave> <true\|false>` | Define uma chave de liga e desliga (aceita também `sim` e `não`) |
| `sandmux config unset <chave>` | Apaga uma chave de liga e desliga |
| `sandmux config add <chave> <item>` | Acrescenta um item a `ro`, `rw` ou `pass-env` |
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
