# Proposal

## Why

O roadmap pede o registro de todo acesso a arquivo do agente e, no change seguinte (16), uma política de arquivos que muda com o agente em execução. O Landlock não muda depois de aplicado, então os dois precisam de um ponto que veja cada abertura na hora. Em 2026-11-14 o usuário escolheu o seccomp user notify, depois da medição:

- seccomp com o supervisor abrindo no lugar do processo: 1,0x a 1,3x no uso normal (opencode, hermes, `cargo check`);
- FUSE de passagem: 2x a 7x nas pastas do agente.

O usuário também corrigiu o desenho no mesmo dia: o supervisor abre toda abertura de arquivo, e não só as das pastas do agente. Deixar o processo abrir sozinho tem a corrida em que outra thread troca o caminho depois da checagem.

## What Changes

- **Aberturas pelo supervisor:**
  - um filtro seccomp com notificação (`SECCOMP_RET_USER_NOTIF`) manda ao supervisor cada `open`, `openat`, `openat2` e `creat` do agente;
  - o supervisor lê o caminho, abre o arquivo ele mesmo dentro da raiz do sandbox, confere as regras de leitura e escrita do Landlock pelo caminho do arquivo que de fato abriu, e devolve o descritor ao processo (`SECCOMP_IOCTL_NOTIF_ADDFD`);
  - trocar o caminho na memória depois da leitura não muda o arquivo entregue.
- **Registro de arquivos:**
  - cada acesso distinto às pastas do agente (pasta de trabalho, HOME sintético, pastas extras e pasta de entrada) vira uma linha JSONL em `$XDG_STATE_HOME/sandmux/files/AAAA-MM-DD.jsonl`, com a etiqueta do lançamento, o caminho, o tipo de acesso (leitura, escrita, criação) e o resultado;
  - sistema, toolchains e caches ficam fora do registro (decisão do usuário).
- **Consulta:** `sandmux files [<id>]` mostra o registro, como o `sandmux net`.
- **Retenção:** a mesma do registro de rede (`net-log-days` e `net-log-max`), aplicada também à pasta `files`.

## Capabilities

### New Capabilities

- `file-access-log`: aberturas de arquivo pelo supervisor, registro dos acessos às pastas do agente, consulta e retenção.

### Modified Capabilities

(nenhuma)

## Impact

- **`sandmux-launcher`:** o filtro com notificação no processo do comando, o socket que leva o descritor do filtro ao supervisor e o atendimento das notificações no supervisor (threads próprias).
- **`sandmux-spec`:** `FilesSpec` na especificação (pasta do registro, etiqueta, pasta de trabalho e as pastas do agente).
- **`sandmux-policy`:** a composição preenche o `FilesSpec`.
- **`sandmux`:** `sandmux files`; a retenção da pasta `files` no daemon.
- **Documentação:** `docs/files.md` (novo), `docs/sessions.md` e o README.
