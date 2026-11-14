# Tasks

## 1. Sandbox

- [x] 1.1 `/mnt/sandmux` na composição (tmpfs, Landlock de escrita, `SANDMUX_FOLDERS`, raiz do registro) e as pastas lembradas da aba montadas nele com o `PASTAS.txt`; verificar com testes de unidade da composição
- [x] 1.2 Auxiliar de montagem (user namespace do sandbox, namespace próprio, `open_tree`, `move_mount`, `umount2`) e o socket de controle no supervisor; verificar com teste de integração do lançador (montar e desmontar ao vivo, só leitura e escrita)

## 2. Daemon, linha de comando e TUI

- [x] 2.1 `FolderEdit` e `FolderList` (protocolo 15), as pastas na aba e `sandmux folder add|remove|list`, com as recusas de pasta perigosa; verificar com teste de CLI numa sessão viva
- [x] 2.2 Tela de pastas na TUI (Ctrl+b `o`); verificar com teste de PTY

## 3. Documentação e verificação

- [x] 3.1 Atualizar `docs/files.md`, `docs/tui.md`, `docs/sessions.md` e o README; verificar com o teste de documentação
- [x] 3.2 Rodar `./scripts/check.sh` sem falhas
