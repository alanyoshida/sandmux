# Proposal

## Why

O change 15 pôs toda abertura de arquivo do agente nas mãos do supervisor, sem a corrida do caminho. Falta usar isso para decidir, com o agente rodando:

- negar e liberar arquivos e pastas na hora;
- esconder os arquivos com cara de segredo que já estão na pasta de trabalho (decisão do roadmap: o agente continua vendo os que ele mesmo criou);
- mostrar os acessos novos com uma ação de bloquear;
- fechar a brecha do `.git/commondir` achada no change 1: um `commondir` criado pelo agente faz o git do host usar outro diretório git, com outros hooks.

A pasta nova liberada ao vivo, que exige montar no sandbox em execução, ficou para o change 26.

## What Changes

- **Política de arquivos:**
  - chaves `file-deny` e `file-allow` (padrões de caminho), em qualquer camada, e `--file-deny`/`--file-allow` no `run`;
  - negar vale para abrir em qualquer lugar; `file-allow` vence `file-deny` e o esconder;
  - a política é recompilada pelo daemon e relida pelo supervisor na abertura seguinte, sem relançar o agente.
- **Arquivos com cara de segredo:**
  - na pasta de trabalho, os arquivos da lista padrão aprovada pelo usuário em 2026-11-14 não abrem (`ENOENT`); o nome ainda aparece no `ls` (decisão do change 15);
  - um arquivo assim criado pelo próprio agente continua aberto para ele, inclusive em lançamentos seguintes, e a criação fica no registro;
  - as chaves `secret-files` e `secret-files-off` acrescentam e desligam padrões.
- **`.git/commondir`:** criar, renomear para, ou ligar (link ou symlink) `.git/commondir` na pasta de trabalho é negado. Para isso, `rename`, `renameat`, `renameat2`, `link`, `linkat`, `symlink` e `symlinkat` também passam pelo supervisor, que os faz no lugar do processo.
- **Edição pela linha de comando:** `sandmux files deny|allow|forget <padrão> <id>` muda a aba da sessão, com efeito na abertura seguinte.
- **Tela de arquivos na TUI:** Ctrl+b `f` mostra os acessos da sessão, com os novos (nunca vistos nesta pasta de trabalho) destacados, e `b` bloqueia o caminho escolhido na aba.

## Capabilities

### New Capabilities

- `file-policy`: negar e liberar arquivos na hora, esconder os com cara de segredo, a brecha do `.git/commondir`, a edição pela linha de comando e a tela de arquivos.

### Modified Capabilities

- `config-profiles`: chaves `file-deny`, `file-allow`, `secret-files` e `secret-files-off`.

## Impact

- **`sandmux-spec`:** `FilePolicy` e o arquivo de política no `FilesSpec`.
- **`sandmux-policy`:** as chaves novas, a lista padrão de segredos e a compilação da política.
- **`sandmux-launcher`:** as decisões no supervisor, a releitura da política, os criados pelo agente, os caminhos conhecidos, e o `rename`/`link`/`symlink` no lugar do processo.
- **`sandmux`:** `sandmux files deny|allow|forget`; o daemon grava e recompila a política de arquivos; a tela Ctrl+b `f`.
- **`sandmux-proto`:** ações de arquivo no `NetEdit`; `PROTOCOL` 14.
- **Documentação:** `docs/files.md`, `docs/config.md`, `docs/tui.md` e o README.
