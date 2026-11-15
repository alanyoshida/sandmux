## 1. Importação do shell

- [x] 1.1 `sandmux-import`: tabela do `shell` escolhida pelo `SHELL` (bash, zsh, fish), `Item::Framework` sem `.git`, `cache` e `log`, `fish_variables` de fora; `importable` e `presets::importable` com o `shell`
- [x] 1.2 Segredos: os arquivos de inicialização como `Env`, o formato `Fish` com `set`; trecho `[sandmux]` no fim do arquivo principal
- [x] 1.3 Primeira execução (CLI e TUI): a importação do `shell` substitui o arquivo gerado; sem terminal, o `shell` começa sem importar

## 2. Testes e documentação

- [x] 2.1 Testes unitários: tabela por shell, framework sem `.git`, `starship.toml` de fora, `export` e `set -gx` trocados, trecho do prompt
- [x] 2.2 Testes de integração: `sandmux import shell` com o token falso e a marca no prompt; primeira execução sem terminal sem importar; `--overwrite` num HOME já criado
- [x] 2.3 Documentação: README, `docs/sessions.md`, `docs/vault.md` e os pares em inglês
