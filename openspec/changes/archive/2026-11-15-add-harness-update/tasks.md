## 1. Atualizadores

- [x] 1.1 Tabela de atualizadores em `sandmux-policy` (opencode, hermes, claude, omp), com a checagem de instalação do preset e o erro de harness sem atualizador; testes unitários
- [x] 1.2 Subcomando `sandmux update <harness> [--yes] [-- args]` na CLI: consulta das abas vivas sem subir o daemon, confirmação no terminal, recusa sem terminal, execução em primeiro plano com o código de saída e o aviso de reinício; textos com `tr!`/`trf!`

## 2. Testes e documentação

- [x] 2.1 Testes de integração com atualizadores falsos num HOME temporário: argumentos, `--`, código de saída, `shell`, harness ausente, aba viva sem e com `--yes`
- [x] 2.2 Documentação: `docs/sessions.md`, `docs/claude.md`, `docs/omp.md` e os pares em inglês; os dois READMEs
