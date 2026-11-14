# Design

## Context

- **Change 15:**
  - o supervisor abre toda abertura (`open`, `openat`, `openat2`, `creat`) dentro da raiz do sandbox;
  - ele decide pelo arquivo aberto (o caminho na visão do agente);
  - registra os acessos às pastas do agente em `$XDG_STATE_HOME/sandmux/files/`.
- **Rede (change 9):** o daemon compila a política e a grava num arquivo por lançamento, e o gateway o relê. As edições da aba vão pelo pedido `NetEdit`, e a TUI tem a tela de rede (Ctrl+b `a`).
- **`.git`:** a pasta de trabalho é montada com escrita, e `hooks` e `config` em somente leitura. O `commondir` não existe e não pode ser montado (montar criaria o arquivo no host).

## Goals / Non-Goals

**Goals:**

- Negar e liberar arquivos sem relançar o agente.
- Esconder os segredos da pasta de trabalho e fechar a brecha do `.git/commondir`.
- Mostrar os acessos novos e bloquear com uma tecla.

**Non-Goals:**

- Tirar os segredos do `ls` (`getdents`) e do `stat`: o nome continua visível (decisão do change 15).
- A pasta nova ao vivo e a tela de pastas (change 26).
- Negar a execução: o `execve` não passa pelo supervisor.

## Decisions

### 1. Padrões de arquivo

- **Formas:**
  - com `/` no começo ou `~/`: um caminho na visão do agente, com `*` (dentro de um componente), `**` (qualquer número de componentes) e `?`;
  - sem `/`: casa com o nome do arquivo em qualquer pasta, como `*.sql`.
- **Pasta:** um padrão que casa com uma pasta vale para tudo dentro dela.
- **Chaves** (todas listas, somadas entre as camadas como as da rede):
  - `file-deny`: o que não abre;
  - `file-allow`: exceções que vencem `file-deny` e o esconder;
  - `secret-files`: padrões a somar à lista de segredos;
  - `secret-files-off`: padrões da lista padrão a desligar.
- **Lista padrão de segredos** (aprovada pelo usuário em 2026-11-14): `.env` e `.env.*` (menos `.env.example`, `.env.sample` e `.env.template`), `*.pem`, `*.key`, `*.p12`, `*.pfx`, `*.kdbx`, `id_rsa*`, `id_ecdsa*` e `id_ed25519*` (menos os `.pub`), `.npmrc`, `.pypirc`, `.netrc`, `credentials*.json`, `*.tfstate` e `.git-credentials`.

### 2. Decisão no supervisor

Depois de abrir, pelo caminho do arquivo aberto na visão do agente, nesta ordem:

1. casa com `file-allow`: passa;
2. casa com `file-deny`: `EACCES`, registrado como `file-deny: <padrão>`;
3. está na pasta de trabalho, o nome casa com um segredo, e não está entre os criados pelo agente: `ENOENT`, registrado como `segredo`. Uma abertura com `O_CREAT` de um segredo que já existia também falha (`EACCES`), para o agente não sobrescrever o arquivo do usuário;
4. o resto: as regras do Landlock, como no change 15.

- **Criados pelo agente:**
  - um segredo criado pela abertura (com `O_CREAT`, sem existir antes) entra na lista da pasta de trabalho;
  - a lista fica em `$XDG_STATE_HOME/sandmux/files/created/<hash da pasta>.json`, fora do alcance do agente, e vale nos lançamentos seguintes;
  - a criação já vai para o registro (`create`).

### 3. Política ao vivo

- **Tipo:** `FilePolicy { deny, allow, secrets, workdir }` em `sandmux-spec`.
- **Arquivo:** o `FilesSpec` ganha a política inicial e um `policy_file` (`$XDG_RUNTIME_DIR/sandmux/files/<etiqueta>.json`).
- **Daemon:** grava o arquivo ao lançar, e regrava quando a configuração muda ou a aba é editada (o mesmo caminho da rede: `recompile`).
- **Supervisor:** confere o mtime do arquivo no máximo a cada 200 ms e relê quando muda.

### 4. Edição

- **Pedido:** o `NetEdit` ganha as ações `file-deny`, `file-allow` e `file-forget` (protocolo 14), que mudam a camada da aba, como as da rede.
- **Linha de comando:** `sandmux files deny|allow|forget <padrão> <id>`.
- **Opções do `run`:** `--file-deny` e `--file-allow`.

### 5. `.git/commondir` e as chamadas de nome

- **Filtro:** passa a notificar também `rename`, `renameat`, `renameat2`, `link`, `linkat`, `symlink` e `symlinkat`.
- **No lugar do processo, dentro do sandbox:**
  - o supervisor lê os caminhos uma vez e resolve as pastas de origem e de destino dentro da raiz (`openat2` com `RESOLVE_IN_ROOT` e `O_PATH|O_DIRECTORY`); trocar o caminho na memória depois não muda nada;
  - a chamada (`renameat2`, `linkat` ou `symlinkat`, com os descritores das pastas e o último componente) é feita por um auxiliar que entra no user namespace e no mount namespace do sandbox antes da liberação, descarta as capabilities e aplica o mesmo Landlock do comando;
  - assim, o kernel decide os pontos de montagem e as regras de escrita como para o comando.
- **Achado da implementação:** a primeira versão fazia a chamada no próprio supervisor, e o teste de fuga mostrou que ele conseguia renomear um arquivo coberto por montagem (`credentials.toml`) e o `.git`. Fora do mount namespace do sandbox, o kernel não vê os pontos de montagem de lá, e o Landlock não vale para o supervisor.
- **Negação:** um destino `.git/commondir` sob a pasta de trabalho é negado (`EACCES`), assim como abrir esse caminho com `O_CREAT`.
- **Denylist no destino:** o `file-deny` também vale para o destino de um rename ou link, para o agente não contornar a negação trocando o nome.

### 6. Tela de arquivos

- **Abrir:** Ctrl+b `f` abre a tela, que lista os acessos recentes da sessão (do registro).
- **Novo:** um caminho nunca visto nesta pasta de trabalho, marcado com `•`.
- **Caminhos conhecidos:** ficam em `files/known/<hash>`, gravados pelo supervisor, como os domínios conhecidos da rede.
- **Teclas:** `b` nega o caminho escolhido na aba (`file-deny` do caminho exato); `l` libera (`file-allow`); Esc fecha.

## Risks / Trade-offs

- **[Nome visível]** O agente vê no `ls` que existe um `.env`. → O conteúdo não abre; a documentação cita.
- **[Rename no lugar do processo]** O supervisor faz o rename sem o Landlock. → Ele confere as regras de escrita das duas pastas, e os testes de fuga cobrem.
- **[Criados pelo agente]** Um `.env` criado pelo agente e depois preenchido pelo usuário continua aberto para o agente. → Documentado; `sandmux files deny` resolve.
- **[Custo]** As chamadas de nome passam pelo supervisor. → São raras perto das aberturas.

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart` (protocolo 14). Os segredos da pasta de trabalho passam a ficar escondidos nos lançamentos novos.
