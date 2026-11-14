# Tasks

## 1. Mecanismo

- [x] 1.1 Instalar no processo do comando o filtro com notificação (`open`, `openat`, `openat2`, `creat`) e levar o descritor dele ao supervisor por um socketpair criado antes do `clone`; verificar com teste de unidade do filtro e com um lançamento que abre arquivos
- [x] 1.2 Atender as notificações no supervisor (quatro threads): ler o caminho e a `open_how`, resolver na visão do agente (absoluto com `RESOLVE_IN_ROOT`, relativo pelo diretório, links mágicos do próprio processo, recusa dos de outros processos), aplicar a `umask` e entregar com `ADDFD`; verificar com testes de integração (link absoluto, `<(...)`, `/proc/self`, `openat2`, criação com `umask`)
- [x] 1.3 Conferir as regras do Landlock pelo caminho do arquivo aberto; verificar com testes de unidade da conferência e com o teste de fuga (`escape_suite`) passando

## 2. Registro

- [x] 2.1 Acrescentar `FilesSpec` à especificação e preenchê-lo na composição (pastas do agente); gravar as linhas no supervisor (uma por caminho e acesso por lançamento, bloqueios sempre); verificar com testes de unidade e de integração
- [x] 2.2 `sandmux files [<id>] [--days N]` e a retenção da pasta `files` no daemon; verificar com teste de CLI

## 3. Documentação e verificação

- [x] 3.1 Escrever `docs/files.md` e atualizar `docs/sessions.md` e o README; verificar com o teste de documentação
- [x] 3.2 Rodar `./scripts/check.sh` sem falhas e medir à mão o opencode e o hermes reais no sandbox, com e sem o filtro
