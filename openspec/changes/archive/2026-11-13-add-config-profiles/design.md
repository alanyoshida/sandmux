# Design

## Context

Veja proposal.md, seção Why. O estado atual que importa para este change:

- **Opções do `run`.** As opções viram `LaunchOptions` (crate `sandmux-proto`), com `kvm`, `no_gpu`, `allow_clipboard` e `repo` como booleanos. "Falso" ali quer dizer "não pedido", e não existe forma de dizer "desligado de propósito".
- **Composição.** O `run` direto e o daemon chamam o mesmo `launch::prepare(host, opts)`, que converte `LaunchOptions` em `RunOptions` e chama `sandmux_policy::compose`. O `allow_clipboard` não passa pela composição: o daemon usa o valor para ligar o filtro de OSC 52 da sessão.
- **Pastas recusadas.** `check_dangerous` recusa, como pasta de trabalho e em `--ro` e `--rw`, a raiz, o HOME, uma pasta que contém o HOME e o diretório de dados (dentro ou contendo).
- **Daemon.**
  - Compõe as sessões, inclusive na restauração sem cliente.
  - O cliente envia na saudação o diretório de dados, e o daemon recusa um diferente.
  - A lista de sessões (`sessions.json`) guarda `LaunchOptions` inteiras.
  - A unidade systemd leva `XDG_DATA_HOME`.
- **Restart.** O `sandmux restart` pega os nomes de `pass-env` da `SessionInfo` e envia os valores do ambiente do terminal.
- **Dependências.** Não existe leitor de TOML no workspace. O `toml` 0.9 está no cache local do cargo, o `toml_edit` não.

## Goals / Non-Goals

**Goals:**

- Uma única função resolve a política efetiva, a partir do arquivo, do harness, da pasta, do perfil e das opções do `run`. O `run` direto, o daemon, o `restart`, a restauração e o `config show` usam todos ela.
- A política efetiva vira as mesmas `RunOptions` de hoje. A composição (`compose`) não muda de forma e não conhece perfis.
- O arquivo editado à mão e o editado por comando são o mesmo arquivo, sem formato intermediário e sem cache.

**Non-Goals:**

- Desligar itens da baseline. As chaves desta fatia são as do proposal.
- Recarregar a configuração numa sessão viva. A política de um sandbox é fixada no lançamento, como hoje.
- Variáveis de ambiente apontando outros arquivos de configuração, inclusão de arquivos, herança entre perfis.
- Mover o `--history-lines` do `sandmuxd` para o arquivo.

## Decisions

### 1. Módulo `config` no `sandmux-policy`

O módulo novo `sandmux_policy::config` tem três partes:

- **Leitura e validação:** `Config::load(path)` devolve um `Config` ou um `ConfigError` com o arquivo, a linha e o motivo.
- **Resolução:** `resolve(&Config, &Host, &Request)` devolve uma `Effective`. O `Request` tem o harness, a pasta, o perfil nomeado e a camada do `run`.
- **Edição:** abre o documento, aplica uma operação, valida o resultado e grava.

Fica no `sandmux-policy` porque é política e porque o daemon e o cliente já dependem dele.

Alternativa descartada: um crate próprio. Ele só teria um usuário e duplicaria `Host` e a expansão de `~/`.

### 2. Modelo de dados

- Cada seção vira uma `Layer` com `ro`, `rw` e `pass_env` (listas) e com `repo`, `kvm`, `gpu` e `clipboard` como `Option<bool>`.
- `Config` tem `global: Layer`, `profiles: BTreeMap<String, Layer>` e `projects: BTreeMap<PathBuf, Layer>`.
- A camada do `run` é uma `Layer` também. Por isso `LaunchOptions` troca os booleanos por `Option<bool>` (`Some(true)` em `--kvm`, `Some(false)` em `--no-kvm`, `None` sem opção) e ganha `profile: Option<String>`. O `no_gpu` vira `gpu: Option<bool>`.
- `Effective` guarda, para cada chave, o valor e a origem: baseline, preset, global, perfil `<nome>`, projeto `<pasta>` ou opção do `run`. Nas listas, cada item guarda a origem dele.
- A origem é o que o `config show` mostra. A composição só usa os valores, por meio de `Effective::run_options(layer_mode)`.
- A camada do preset existe no modelo, mas nenhum preset define chave hoje. Ela fica para quando um preset precisar, por exemplo um harness que exija a GPU desligada.

### 3. Validação e mensagens com linha

A leitura usa o `toml_edit`. O documento imutável guarda a posição de cada item, o que permite apontar a linha também nos erros de sentido: seção desconhecida, chave desconhecida, tipo errado, caminho relativo, nome de variável inválido, `ro` e `rw` repetidos na mesma seção, e projeto com caminho relativo.

Regras:

- **Caminhos:** absolutos ou começando por `~/`. O `~/` é expandido com o HOME do `Host` na resolução, nunca na gravação.
- **Nome de variável:** `[A-Za-z_][A-Za-z0-9_]*`, a mesma regra do `--pass-env`.
- **Nome de perfil:** `[a-z0-9][a-z0-9_-]*`, para caber em mensagens e na lista de sessões sem escape.
- **Pasta de projeto:** caminho absoluto. Não precisa existir: um projeto apagado continua no arquivo e o `config list` marca.

Alternativa descartada: o crate `toml` com `serde` e `deny_unknown_fields`. Ele dá a linha só nos erros de sintaxe e de tipo, e a edição ainda precisaria do `toml_edit`. Com um crate só, a leitura e a escrita entendem o arquivo da mesma forma.

### 4. Resolução

A ordem de aplicação é baseline, preset, global, perfil nomeado, projeto e `run`.

- **Liga e desliga:** a última camada que define a chave vence.
- **Listas:** os itens se somam. Para cada caminho que aparece em `ro` e em `rw`, a camada mais específica decide. Duplicatas idênticas são unificadas.
- **Perfil do projeto:** a pasta de trabalho é canonizada (links resolvidos), e as chaves de projeto também, quando existem. Vale a chave com o maior prefixo, comparado por componentes do caminho e não por texto, para que `/home/ana/proj` não case com `/home/ana/proj2`.
- **Perfil nomeado** inexistente é erro, com a lista dos perfis.
- **Resultado:** a resolução devolve `RunOptions`, com `ro`, `rw` e `pass_env` expandidos e `kvm`, `no_gpu` e `repo` já decididos, e devolve o `clipboard` à parte, como hoje.
- **Verificações:** `compose` continua fazendo as de caminho (existência, pasta perigosa, aviso de credencial) com as listas já somadas. Um caminho da configuração passa pelas mesmas recusas e avisos de um caminho da linha de comando.

### 5. Pasta de configuração e escrita do agente

- `Host` ganha `config_dir`: `$XDG_CONFIG_HOME/sandmux`, ou `~/.config/sandmux`, canonizada quando existe (senão, normalizada), para comparar com os caminhos canônicos da composição.
- `check_dangerous` ganha o parâmetro `writable`. Com escrita (pasta de trabalho e `rw`), também recusa a pasta de configuração, uma pasta dentro dela ou uma que a contém.
- Sem escrita, a pasta de configuração passa. Assim continua valendo o cenário de `--ro ~/.config` da spec `sandbox-filesystem`.
- A regra do diretório de dados continua valendo com e sem escrita, como hoje.

### 6. Quem resolve, e quando

- **Daemon.** Resolve com autoridade na criação, no reinício e na restauração, sempre lendo o arquivo naquele momento. Não há cache: o arquivo é pequeno, e reler evita a dúvida sobre qual versão valeu.
- **Cliente, no `run` com terminal e no `restart`.** Também resolve, por três motivos:
  - recusar antes de pedir a sessão (arquivo inválido, perfil inexistente);
  - saber quais valores de `pass-env` enviar;
  - mostrar os avisos.
- **Arquivo alterado entre as duas leituras.** O daemon pode pedir uma variável que o cliente não enviou. Nesse caso ele recusa com o erro de variável ausente, que já existe, e nada é lançado. Repetir o comando resolve.
- **Restauração.** O daemon resolve sem cliente. Se a política efetiva tem `pass-env`, se o perfil sumiu ou se o arquivo é inválido, a sessão fica parada com o motivo. A mesma função dá o motivo nos três casos.
- **`SessionInfo`.** O `pass_env` passa a conter os nomes do último lançamento resolvido, e a mensagem ganha as opções dadas no `run` e o perfil (`opts`), para o `sandmux restart` resolver as camadas no cliente sem pedido novo. A lista de sessões guarda as opções dadas e o perfil, nunca a política resolvida.

### 7. Protocolo 2 e lista de sessões

- **`PROTOCOL = 2`**, porque `LaunchOptions` muda de forma. A saudação ganha `config_dir`, e o daemon recusa um cliente com outra pasta de configuração, com a mesma mensagem usada para o diretório de dados.
- **Unidade systemd.** `unit_text` passa a escrever `Environment=XDG_CONFIG_HOME=...` quando o cliente tem a variável definida. Uma unidade gravada antes, sem a linha, é regravada pela regra que já existe, a que regrava quando o conteúdo muda.
- **`sessions.json`.** Ganha `"version": 2`. Na leitura, um arquivo sem versão é o formato do change 2:
  - `kvm`, `allow_clipboard` e `repo` verdadeiros viram `Some(true)`;
  - os falsos viram `None`, porque falso significava "não pedido";
  - `no_gpu` verdadeiro vira `gpu: Some(false)`;
  - `profile` fica `None`.

  O arquivo é regravado no formato novo na primeira mudança.

### 8. Edição com `sandmux config`

- **Operações:** `set`, `unset`, `add` e `remove` abrem o arquivo como documento editável do `toml_edit`. Um arquivo inexistente vira documento vazio.
- **Alvo:** cria a seção quando falta. `--project` sem pasta usa a pasta atual canonizada.
- **Validação:** cada operação aplica a mudança e passa o texto resultante pela mesma validação da leitura, mais a verificação de pasta perigosa para `rw`. Só então grava num arquivo temporário na mesma pasta e faz `rename`.
- **Limpeza:** uma chave sem valor e uma seção vazia são retiradas.
- **Permissões:** o arquivo é criado com modo 600 e a pasta com 700; um arquivo existente mantém o modo dele.
- **Formatação:** o que não foi tocado sai byte a byte igual.
- **Valores:** `set` aceita `true` e `false`, e também `sim` e `não`, por ser uma CLI em português. A gravação é sempre `true` ou `false`.
- **Itens de lista:** `add` de um item que já existe não muda nada e avisa; `remove` de um item ausente é erro.

Alternativa descartada: só a edição à mão. Foi preterida pelo usuário, porque a TUI vai precisar das mesmas operações.

### 9. `config show` e `config list`

- **`show`:** uma tabela com CHAVE, VALOR e ORIGEM, e uma linha por item nas listas. A origem é "padrão" para baseline e preset, e "modificada (global)", "modificada (perfil vm)", "modificada (projeto /home/ana/proj)" ou "modificada (opção do run)" nos demais casos. Não há opção do `run` no `show`, então essa última origem não aparece ali, mas a função é a mesma.
- **`list`:** os perfis e as pastas de projeto, cada um com as chaves que define, no formato do TOML, e "(pasta não existe)" quando for o caso.
- **Arquivo inválido:** os dois comandos terminam com o erro e a linha.

### 10. Opções negativas do `run`

Cada opção nova faz par com a positiva e conflita com ela no `clap` (`conflicts_with`): `--no-kvm`, `--gpu`, `--no-allow-clipboard` e `--no-repo`. Com isso, a camada do `run` vira um `Option<bool>` sem ambiguidade.

### 11. Diagnóstico

- O `doctor` ganha a seção "Configuração", com o caminho e o estado: não existe, válido (com o número de perfis e de projetos) ou inválido (com a linha e o motivo).
- O `doctor` não cria nem altera o arquivo.

## Risks / Trade-offs

- **[Perfil afrouxa sem a pessoa notar]** Quem liga `clipboard` ou um `rw` no `[global]` esquece que vale para toda pasta. → O `run` repete os avisos de sempre (credencial montada, `pass-env`), e o `config show` mostra a origem de cada chave.
- **[Erro de digitação bloqueia tudo]** Um arquivo inválido impede todo lançamento e para as sessões na restauração. → A mensagem traz o arquivo e a linha, e o `doctor` mostra o mesmo. Ignorar uma parte do arquivo seria pior, porque o usuário acharia que uma restrição vale quando não vale.
- **[Mudança entre as leituras do cliente e do daemon]** → O daemon tem a palavra final e recusa sem lançar. Descrito na decisão 6.
- **[Outros arquivos que dão execução fora do sandbox]** `--rw` sobre `~/.config/systemd/user`, `~/.bashrc` ou `~/.profile` deixa o agente plantar algo que roda no host. Este change só fecha a pasta de configuração do sandmux, e a recusa de `--rw ~/.config` fecha também o systemd de usuário. → O `~/.bashrc` e afins ficam como risco anotado no roadmap, para a etapa 6.
- **[Volta para a versão anterior]** O daemon do change 2 não lê o `sessions.json` novo. → Documentado em `docs/config.md`. Voltar de versão perde a lista de sessões, mas não afeta sandbox nem HOME sintético.
- **[Dependência nova]** `toml_edit`, mantido pelo mesmo grupo do `cargo`, sem código nativo. → Versão fixada no `Cargo.lock`, como as demais.

## Migration Plan

1. Atualizar o binário.
2. O cliente novo recusa o daemon antigo pela verificação de protocolo e indica `sandmux daemon restart`.
3. O `daemon restart` sobe o daemon novo, que lê a lista antiga, converte e relança as sessões vivas. Sem arquivo de configuração, nada muda.
4. A unidade systemd é regravada no próximo uso, se o cliente tiver `XDG_CONFIG_HOME`.

Rollback: instalar o binário anterior e rodar `sandmux daemon restart`. A lista de sessões se perde, e o arquivo de configuração é ignorado.
