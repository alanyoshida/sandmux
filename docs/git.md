# Git dentro do sandbox

O sandbox protege os pontos do `.git` da pasta de trabalho que o git do host executa ou obedece. O agente faz commit, branch, merge e stash normalmente; o que ele não consegue é plantar um hook ou alterar a configuração do repositório.

## O que fica protegido

| Caminho | Proteção |
|---|---|
| `.git` | Ponto de montagem: não pode ser renomeado, apagado nem substituído |
| `.git/hooks` | Somente leitura |
| `.git/config` | Somente leitura |
| `.git/modules/<submódulo>` | O mesmo tratamento do `.git`: a pasta, os hooks e a configuração de cada submódulo |
| Arquivo `.git` de um worktree | Somente leitura, para não poder apontar para outro diretório git |

O commit sai com o nome e o e-mail do seu git: ao criar o HOME sintético, o sandmux copia só `user.name` e `user.email`. Aliases, credential helpers e URLs com token ficam de fora.

## Operações do dia a dia

Resultado medido em 2026-11-12, com git 2.x, rodando dentro do sandbox num repositório com um remoto.

| Operação | Resultado |
|---|---|
| `git commit`, `git branch`, `git merge`, `git rebase`, `git stash` | Funciona |
| `git fetch`, `git pull` | Funciona |
| `git gc` | Funciona |
| `git worktree add` | Funciona |
| `git init` numa subpasta | Funciona, mas o repositório novo não tem proteção nenhuma (ver limites) |
| `git config --global ...` | Funciona: grava no `.gitconfig` do HOME sintético, que vale só dentro do sandbox |
| `git config <chave> <valor>` (local) | **Falha** com `could not write config file .git/config` |
| `git remote add` | **Falha** com a mesma mensagem |
| `git checkout -b x origin/x` | Cria a branch, mas **não grava o rastreio**. O git mostra o erro e termina com sucesso |
| `git branch --set-upstream-to=...` | **Não grava**. O git mostra o erro, diz que configurou o rastreio e termina com sucesso |
| `git push -u` | Envia, mas **não grava** o upstream, como acima |
| `git submodule add` | Cria `.gitmodules`, a pasta e `.git/modules/<nome>`, mas **não grava** o submódulo em `.git/config` |

Nos casos em que o git termina com sucesso sem gravar, a mensagem `error: could not write config file .git/config` aparece antes. Para configurar rastreio de branch, remotos ou submódulos, faça a operação no host.

## Limites conhecidos

- **`.git/commondir`**: um arquivo `commondir` criado pelo agente dentro do `.git` faz o git do host passar a usar outro diretório git, com outra configuração e outros hooks. A proteção por montagem não impede a criação desse arquivo. Confirmado em 2026-11-12. A correção chega com a interceptação de arquivos da etapa 6 (decisão de 2026-11-13). Até lá, antes de usar o git no host depois de uma execução, confira se existe `.git/commondir` num repositório que não é worktree e apague o arquivo se o agente o criou.
- **Repositório criado durante a execução**: um `git init` ou um `git submodule add` feito pelo agente gera um diretório git sem proteção até o próximo lançamento. Quem rodar git no host dentro dessa pasta obedece ao que o agente gravou ali.
- **`core.hooksPath` apontando para uma pasta versionada** (caso do husky): os hooks moram na pasta de trabalho e continuam graváveis.
- **Outros arquivos que o host executa**: `Makefile`, scripts do `package.json`, `.envrc`, tarefas do editor. O que o agente escreve ali precisa ser lido antes de ser executado fora do sandbox.
