## Context

A importação (`sandmux-import`) tem uma tabela estática por harness, um `scrub` por formato decidido pelo nome do arquivo (`format_of`) e um manifesto das trocas. O preset `shell` não tinha tabela: `importable("shell")` era falso, e o HOME sintético recebia só o arquivo mínimo de `write_shell_rc`, com o prompt `[sandmux]`. O shell do preset vem de `system_shell(host)`, que lê o `SHELL` do usuário.

O shell do usuário nesta máquina é o `bash`, com `~/.bashrc` e `~/.profile`.

## Goals / Non-Goals

**Goals:**
- O shell confinado com os aliases, funções e o framework do usuário.
- Nenhuma credencial real no HOME sintético do `shell`.
- Scripts que já usam `sandmux run shell` sem terminal continuam funcionando.

**Non-Goals:**
- Importar os arquivos de outros shells além do que o preset usa.
- Programas que o shell chama (`starship`, `direnv`, `fzf`): os binários e as configurações deles ficam de fora.
- Importação sincronizada (trazer o que mudou a cada execução), que é outro item da lista.

## Decisions

- **Tabela pelo `SHELL`:** `table("shell")` lê o `SHELL` do processo, com a mesma regra de `system_shell` (nas pastas `/usr/` ou `/bin/`, e o arquivo existe; senão, `bash`), e escolhe a tabela pelo nome do arquivo (`bash`, `zsh`, `fish`; qualquer outro usa só o `.profile`). Quem chama a importação é o cliente (CLI ou TUI), que é o mesmo processo cujo `SHELL` o `Host::detect` lê para lançar o preset. A assinatura das funções públicas não muda.
- **Itens novos:** `Item::Framework(pasta)`, uma pasta copiada sem `.git`, `cache` e `log` em qualquer nível. Um nome de arquivo a pular, `fish_variables`, que guarda as variáveis universais do fish, inclusive as exportadas.
- **Formato:** `format_of` trata como `Env` os nomes exatos dos arquivos de inicialização (`.bashrc`, `.bash_aliases`, `.bash_profile`, `.bash_login`, `.bash_logout`, `.profile`, `.zshrc`, `.zshenv`, `.zprofile`, `.zlogin`, `.zlogout`). A regra `Env` já aceita `export NOME=valor` e ignora `$(...)` e `$VAR`. Arquivos `.fish` ganham o formato `Fish`, com `set [-opções] NOME valor`. Os demais arquivos (frameworks, `.inputrc`, `.p10k.zsh`) são `Text`: só a troca de valores com formato de token.
- **Marca no prompt:** depois do `scrub`, o arquivo principal (`.bashrc`, `.zshrc`, `.config/fish/config.fish`) ganha um trecho marcado: no `bash`, `PS1="[sandmux] $PS1"`; no `zsh`, `PROMPT="[sandmux] $PROMPT"`; no fish, uma cópia do `fish_prompt` do usuário chamada por um `fish_prompt` novo que escreve `[sandmux] ` antes. O mesmo trecho entra a cada importação, então a comparação da reimportação continua estável.
- **Primeira execução:** o HOME sintético acabou de ser criado e só tem o arquivo gerado. A importação do `shell` na primeira execução usa `overwrite`, e o arquivo real substitui o gerado. Os outros presets seguem como hoje.
- **Sem terminal:** o `shell` começa sem importar, em vez de exigir `--import`/`--no-import`. O preset era usado em scripts antes de ter importação, e o erro quebraria esses scripts. `--import` continua valendo.
- **Fora do vigia de chaves:** o `shell` não entra em `SCANNED`. O usuário escreve no `.bashrc` do HOME sintético de propósito, e o vigia é para os arquivos que os harnesses gravam.

## Risks / Trade-offs

- [Um valor que não é segredo, com nome de credencial (`export SSH_AUTH_SOCK=/run/...`), vira falso] → Na dúvida, a regra da importação é trocar. O usuário ajusta no HOME sintético.
- [Um `.bashrc` que chama programas fora do sandbox (`starship init`, `pyenv`)] → O comando falha dentro do sandbox, com a mensagem do próprio shell; o resto do arquivo roda.
- [O `SHELL` do cliente e o do lançamento diferentes] → Os dois vêm do mesmo processo do cliente. Uma troca de shell depois da importação deixa os arquivos do shell anterior no HOME sintético; `sandmux import shell` traz os do novo.
- [Framework grande] → O `.git` fica de fora, que é a maior parte do oh-my-zsh.
