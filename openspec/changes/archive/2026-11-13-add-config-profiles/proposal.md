# Proposal

## Why

Hoje toda exceção à baseline é uma opção do `sandmux run` (`--ro`, `--rw`, `--kvm`, `--no-gpu`, `--allow-clipboard`, `--pass-env`, `--repo`). Ela precisa ser repetida a cada execução e some quando o usuário esquece de digitá-la. O roadmap decidiu um arquivo TOML com camadas de política, perfis nomeados e um perfil por projeto ligado ao caminho da pasta, nunca lido de dentro dela. Este change entrega isso antes da TUI (change 5), que vai usar as mesmas camadas para a tela de gerenciamento, e antes das etapas de rede e arquivos, que vão acrescentar listas às mesmas camadas.

## What Changes

- **Arquivo de configuração** `$XDG_CONFIG_HOME/sandmux/config.toml` (por padrão `~/.config/sandmux/config.toml`). Ele guarda só o que difere do padrão e tem três tipos de seção:
  - `[global]`, que vale para toda execução;
  - `[profile.<nome>]`, um perfil nomeado;
  - `[project."<pasta>"]`, o perfil de um projeto, ligado ao caminho absoluto da pasta.
  - O arquivo pode não existir, e isso equivale a não haver ajuste nenhum. Um arquivo inválido (TOML malformado, chave desconhecida, valor de tipo errado, caminho relativo) faz o `run` recusar o lançamento, com o arquivo, a linha e o motivo na mensagem.
- **Chaves desta fatia**, as mesmas das opções do `run`:
  - `ro` e `rw`: listas de caminhos absolutos ou começando por `~/`;
  - `pass-env`: lista de nomes de variáveis, nunca valores;
  - `repo`, `kvm`, `gpu` e `clipboard`: liga e desliga.
- **Camadas**, da menos para a mais específica:
  1. baseline embutida;
  2. preset do harness;
  3. `[global]`;
  4. perfil nomeado;
  5. perfil do projeto;
  6. opções do `run`.
  - Nas chaves de liga e desliga, vence a camada mais específica que definiu a chave.
  - As listas se acumulam entre as camadas. Se o mesmo caminho aparece em `ro` numa camada e em `rw` noutra, vale a camada mais específica.
  - Neste change, os perfis só acrescentam ou ajustam essas chaves. Os itens da baseline (arquivos de `/etc` cobertos, toolchains, caches, credenciais conhecidas) continuam fixos. Desligar item da baseline fica para quando houver tela de gerenciamento (etapa 6).
- **Seleção de perfil:**
  - `sandmux run --profile <nome>` aplica um perfil nomeado. Um nome que não existe faz o `run` terminar com erro, listando os perfis.
  - O perfil do projeto entra sozinho quando a pasta de trabalho é a pasta ligada a ele ou uma subpasta dela. Com perfis em pastas aninhadas, vale só o da pasta mais próxima.
- **Opções negativas no `run`**, para que a linha de comando também possa desligar o que um perfil ligou: `--no-kvm`, `--gpu`, `--no-allow-clipboard` e `--no-repo`. As atuais (`--kvm`, `--no-gpu`, `--allow-clipboard`, `--repo`) continuam.
- **Comandos `sandmux config`:**
  - `show` mostra a configuração efetiva para a pasta atual, ou para `--workdir` e `--profile`. Cada chave aparece marcada como "padrão" ou "modificada", com a camada de onde veio.
  - `set` e `unset` mudam uma chave de liga e desliga.
  - `add` e `remove` mudam um item de lista.
  - `list` mostra os perfis nomeados e as pastas com perfil de projeto.
  - Os comandos de escrita exigem o alvo explícito: `--global`, `--profile <nome>` ou `--project [<pasta>]`. Eles gravam só a diferença e preservam comentários e a formatação do que o usuário editou à mão.
- **Política resolvida a cada lançamento.**
  - As sessões guardam as opções dadas no `run` e o nome do perfil, e não a política resolvida.
  - Criar, reiniciar e restaurar uma sessão resolvem as camadas de novo, com a configuração daquele momento. Uma mudança no arquivo vale a partir do próximo lançamento de cada sessão.
  - Uma sessão cujo perfil nomeado sumiu, ou cuja configuração ficou inválida, fica parada na restauração, com o motivo.
  - Uma sessão cuja configuração efetiva passou a ter `pass-env` também fica parada, como já acontece com quem usa a opção.
- **A configuração fica fora do alcance do agente.**
  - A pasta de configuração do sandmux não pode ser gravada pelo sandbox. São recusados como pasta de trabalho, ou em `--rw`, a própria pasta, uma pasta dentro dela ou uma pasta que a contém, pela mesma regra do diretório de dados.
  - Em somente leitura ela pode entrar, porque não guarda segredo. Assim `--ro ~/.config` continua funcionando.
- **Daemon:**
  - A unidade systemd passa a levar `XDG_CONFIG_HOME` quando está definido.
  - A saudação passa a conferir também a pasta de configuração do cliente.
  - O protocolo passa à versão 2, porque as opções de sessão mudam de forma.
  - A lista de sessões gravada pela versão anterior é lida e convertida.
- **Diagnóstico:** o `sandmux doctor` mostra o caminho do arquivo de configuração e se ele é válido.

A persistência das abas, prevista para este change no roadmap, já existe desde o change 2: a lista de sessões guarda cada sessão com as suas opções. Os nomes de aba e o que for só da TUI chegam com o change 5. O limite do histórico continua como opção do `sandmuxd` (`--history-lines`), fora do arquivo.

## Capabilities

### New Capabilities
- `config-profiles`: arquivo de configuração, camadas de política, perfis nomeados e de projeto, regra de precedência, comandos `sandmux config` e a proteção da configuração contra o agente.

### Modified Capabilities
- `sandbox-filesystem`: "Dispositivos mínimos, com GPU" passa a aceitar `kvm` e `gpu` vindos da configuração efetiva. A recusa de escrita na pasta de configuração fica na capacidade nova, para não espalhar a regra.
- `terminal-sessions`:
  - "Verificações do run antes de criar a sessão" inclui o arquivo inválido e o perfil inexistente;
  - "Reiniciar uma sessão" passa a resolver as camadas de novo;
  - "Escrita no clipboard bloqueada por padrão" passa a aceitar a liberação pela configuração efetiva.
- `session-daemon`:
  - "Lista de sessões guardada sem segredos" guarda o perfil e as opções dadas, e lê a lista da versão anterior;
  - "Restauração depois de reboot" resolve as camadas de novo e para a sessão quando a configuração não permite relançá-la.

  A unidade systemd com `XDG_CONFIG_HOME` e a saudação que confere a pasta de configuração ficam na capacidade nova ("Cliente e daemon com a mesma configuração").

## Impact

- **`sandmux-policy`:** módulo novo de configuração, com a leitura, a validação, as camadas e a resolução da política efetiva; `check_dangerous` passa a conhecer a pasta de configuração quando o acesso é de escrita. `Host` ganha o caminho da configuração.
- **`sandmux-proto`:** `LaunchOptions` troca os booleanos por valores opcionais (definido ou não pelo `run`) e ganha `profile`. `PROTOCOL` passa a 2.
- **`sandmux`:**
  - o `run` ganha `--profile` e as opções negativas;
  - entra o subcomando `config`;
  - o daemon resolve as camadas ao criar, reiniciar e restaurar;
  - o cliente resolve as camadas para saber quais valores de `pass-env` enviar;
  - a lista de sessões é convertida da versão anterior;
  - a unidade systemd e o `doctor` mudam;
  - o `run` sem terminal também resolve as camadas.
- **Dependências:** `toml_edit`, que lê e grava o arquivo preservando comentários.
- **Documentação:** `docs/config.md`, novo, e o README.
- **Compatibilidade:** quem não cria o arquivo vê o mesmo comportamento de antes. O cliente novo recusa o daemon antigo pela verificação de protocolo, que indica `sandmux daemon restart`.
