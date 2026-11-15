## Why

Item "Configuração de shell importada" da lista depois da v1, escolhido pelo usuário (2026-11-15).

O preset `shell` abre com um arquivo de configuração mínimo, gerado pelo sandmux. Os aliases, funções, `PATH` e o framework do shell do usuário (oh-my-zsh, oh-my-bash) ficam de fora, e o shell confinado parece outro computador. Os outros presets já têm a importação, com os segredos trocados por valores falsos; o `shell` era a exceção.

## What Changes

- **O preset `shell` passa a ter importação:** `sandmux import shell` e a pergunta da primeira execução, como nos harnesses.
- **Só os arquivos do shell em uso:** o shell do preset (o do usuário, quando está nas pastas de sistema; senão, o `bash`) escolhe a tabela. No `bash`: `.bashrc`, `.bash_aliases`, `.bash_profile`, `.bash_login`, `.bash_logout`, `.profile`, `.inputrc` e as pastas `.oh-my-bash` e `.bash_it`. No `zsh`: `.zshrc`, `.zshenv`, `.zprofile`, `.zlogin`, `.zlogout`, `.p10k.zsh` e `.oh-my-zsh`. No `fish`: `.config/fish` sem o `fish_variables`, e as pastas do oh-my-fish.
- **Frameworks copiados junto,** sem as pastas `.git`, `cache` e `log`.
- **Segredos:** nos arquivos de inicialização (`.bashrc`, `.zshrc` e afins), `export NOME=valor` com nome de credencial vira valor falso, como no `.env` do hermes; no fish, `set -gx NOME valor`. Os arquivos dos frameworks só passam pela troca de valores com formato de token.
- **Marca `[sandmux]` no prompt:** o arquivo principal importado ganha no fim um trecho que põe `[sandmux]` na frente do prompt do usuário, para o shell confinado continuar reconhecível.
- **Primeira execução:** a importação do shell substitui o arquivo mínimo gerado. Sem terminal interativo, o `shell` começa sem importar, em vez de terminar com erro, para não quebrar scripts que já usam `sandmux run shell`.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `harness-state-import`: o preset `shell` tem importação, com a tabela do shell em uso; a primeira execução sem terminal do `shell` começa sem importar.
- `harness-presets`: os arquivos de configuração reais do shell entram no preset `shell` pela importação, com os segredos trocados.

## Impact

- **`sandmux-import`:** a tabela do shell, escolhida pelo `SHELL`; pastas de framework sem `.git`, `cache` e `log`; formato dos arquivos de inicialização; trecho do prompt.
- **`sandmux-import::secrets`:** os nomes dos arquivos de inicialização como `Env`, e o formato `Fish`.
- **`sandmux-policy`:** `presets::importable` com o `shell`.
- **CLI e TUI:** a primeira execução do `shell` importa por cima do arquivo gerado; sem terminal, começa sem importar.
- **Testes:** importação com o token trocado, o framework copiado sem `.git`, o `starship.toml` de fora, a marca no prompt; primeira execução sem terminal.
- **Documentação:** README, `docs/sessions.md`, `docs/vault.md` e os pares em inglês.
