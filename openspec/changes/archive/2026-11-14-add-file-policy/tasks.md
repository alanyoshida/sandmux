# Tasks

## 1. Política

- [x] 1.1 Padrões de arquivo (formas, pasta, `**`), a lista padrão de segredos e `FilePolicy` em `sandmux-spec`; chaves `file-deny`, `file-allow`, `secret-files` e `secret-files-off`, e `--file-deny`/`--file-allow` no `run`; verificar com testes de unidade
- [x] 1.2 Compilar a política, gravá-la por lançamento no daemon e regravá-la na mudança da configuração; verificar com testes de unidade

## 2. Supervisor

- [x] 2.1 Decidir as aberturas (allow, deny, segredo, Landlock), reler o arquivo de política e guardar os criados pelo agente e os caminhos conhecidos; verificar com testes de integração (negar na hora, segredo escondido, segredo criado pelo agente)
- [x] 2.2 Notificar e fazer no lugar do processo `rename`, `renameat`, `renameat2`, `link`, `linkat`, `symlink` e `symlinkat`, negando o destino `.git/commondir` e o `file-deny`; verificar com testes de integração e com o teste de fuga

## 3. Edição e tela

- [x] 3.1 Ações `file-deny`, `file-allow` e `file-forget` no `NetEdit` (protocolo 14) e `sandmux files deny|allow|forget`; verificar com teste de CLI (negar com a sessão viva)
- [x] 3.2 Tela de arquivos na TUI (Ctrl+b `f`): acessos recentes, novos destacados, `b` bloqueia e `l` libera; verificar com teste de PTY

## 4. Documentação e verificação

- [x] 4.1 Atualizar `docs/files.md`, `docs/config.md`, `docs/tui.md`, `docs/sessions.md` e o README; verificar com o teste de documentação
- [x] 4.2 Rodar `./scripts/check.sh` sem falhas
