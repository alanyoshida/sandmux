# Proposal

## Why

Hoje uma pasta de fora da pasta de trabalho só entra no sandbox por `--ro`/`--rw` ou pela configuração, no lançamento: para liberar uma pasta nova, o agente precisa ser reiniciado. O roadmap pede liberar uma pasta nova na hora, com uma tela de cadastro e gerenciamento de pastas. O usuário decidiu (2026-11-14) que a pasta nova aparece em `/mnt/sandmux/<nome>`, um lugar já liberado para escrita desde o início, para que ler, gravar, criar, apagar e renomear valham na hora.

## What Changes

- **`/mnt/sandmux`:** todo sandbox tem `/mnt/sandmux`, um tmpfs vazio liberado para escrita no Landlock, e a variável `SANDMUX_FOLDERS` com o caminho. As pastas novas são montadas embaixo dele.
- **Montar ao vivo:**
  - `sandmux folder add <id> <pasta> [--rw]` monta a pasta do host em `/mnt/sandmux/<nome>` na sessão viva, em somente leitura ou com escrita, sem relançar o agente;
  - `sandmux folder remove <id> <nome>` desmonta;
  - `sandmux folder list <id>` lista;
  - o nome é o último componente da pasta (com `-2`, `-3` quando repete);
  - as recusas de pasta perigosa do `--rw` valem (HOME, raiz, diretório de dados e configuração do sandmux), e uma pasta com credenciais avisa.
- **Aviso ao agente:** `/mnt/sandmux/PASTAS.txt`, gerado e em somente leitura, lista cada nome com a pasta do host e o modo.
- **Lembradas na aba:** as pastas da aba voltam nos lançamentos seguintes da sessão, no mesmo `/mnt/sandmux/<nome>`.
- **Tela de pastas na TUI:** Ctrl+b `o` lista as pastas da aba; `a` pede uma pasta e o modo e a monta; `x` desmonta a escolhida.
- **Registro e política:** `/mnt/sandmux` entra nas pastas do agente do registro de arquivos, e a política de arquivos (change 16) vale para ela.

## Capabilities

### New Capabilities

- `live-folders`: `/mnt/sandmux`, montar e desmontar pastas ao vivo, o aviso ao agente, as pastas lembradas na aba e a tela de pastas.

### Modified Capabilities

- `sandbox-filesystem`: "Pasta fora da visão" passa a admitir `/mnt/sandmux`; nada do `/mnt` do host aparece.

## Impact

- **`sandmux-launcher`:** um auxiliar de montagem (no user namespace do sandbox, com as capabilities de lá) e o socket de controle por lançamento no supervisor.
- **`sandmux-spec`:** as pastas iniciais e o socket de controle na especificação.
- **`sandmux-policy`:** `/mnt/sandmux` na composição, no Landlock e nas pastas do agente.
- **`sandmux-proto`:** pedidos `FolderEdit` e `FolderList`, as pastas nas opções da aba; `PROTOCOL` 15.
- **`sandmux`:** `sandmux folder`, o daemon repassa ao supervisor e guarda as pastas na aba; a tela Ctrl+b `o`.
- **Documentação:** `docs/files.md`, `docs/tui.md`, `docs/sessions.md` e o README.
