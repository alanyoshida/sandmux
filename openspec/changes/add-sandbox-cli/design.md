# Design

## Context

Motivação e escopo estão em `proposal.md`. Este documento cobre como o confinamento é montado.

Restrições que moldam o desenho:

- **Sem privilégio.** Tudo aqui roda como o usuário comum. O passo de instalação com `sudo` e o eBPF são de changes posteriores.
- **Quatro binários no projeto**: cliente (`sandmux`), daemon de sessões, lançador (`sandmux-run`) e auxiliar root. Este change entrega o lançador e o cliente em forma de linha de comando. O lançador precisa servir, sem mudança, ao daemon que virá depois.
- **Ameaça alvo**: um agente manipulado por prompt injection que tenta ativamente ler segredos e sair do confinamento. Exploit de kernel está fora do escopo.
- **Seguro por padrão, mínimo atrito**: a baseline vem embutida e funciona sem configuração; na dúvida, preferir o padrão que não quebra programas.
- **Estado real na máquina de referência**: opencode é um binário nativo em `~/.opencode/bin` com estado em quatro pastas XDG; hermes é um wrapper shell para uma instalação Python em `~/.hermes/hermes-agent`, com tudo em `~/.hermes` e um gateway próprio com socket. Quase todas as toolchains (nvm, bun, cargo, rustup, uv, pyenv) ficam no HOME real, que a baseline esconde.

## Goals / Non-Goals

**Goals:**

- opencode e hermes sobem e funcionam dentro do sandbox sem configuração manual.
- Nada fora da pasta de trabalho, do HOME sintético e do sistema em somente leitura é alcançável, por arquivo, socket ou processo.
- Cada restrição é aplicada pelo kernel e não depende de cooperação do processo confinado.
- O lançador é pequeno, sem conhecimento de harness ou de política, e auditável isoladamente.
- O uso corriqueiro de quem desenvolve continua funcionando lá dentro: commit no git e instalação global nas toolchains.
- Existe uma suíte de testes de fuga que tenta atravessar cada restrição e precisa falhar em todas.

**Non-Goals:**

- Controlar ou registrar rede; esconder arquivos de segredo da pasta de trabalho; registrar acessos a arquivo; mudar política com o agente rodando.
- Filtrar sequências de escape do terminal (por exemplo escrita no clipboard). Isso exige um PTY intermediário, que é papel do daemon.
- Proteger o próprio estado do harness entre projetos: o HOME sintético é compartilhado por decisão do usuário.
- Arquivo de configuração de perfis. Nesta fatia a política é baseline, preset e opções de linha de comando.
- Rodar contêineres e instalar pacotes do sistema. O socket do Docker do host nunca entra no sandbox; o Docker privado é de outro change.
- Abrir programas com janela. Wayland e X11 ficam fora do sandbox; navegador headless funciona.
- Instalar ou atualizar o harness. Os dois são feitos no host, pelo usuário.

## Decisions

### 1. Lançador recebe uma especificação resolvida

```
sandmux run opencode            (ou, no futuro, o daemon)
      |
      |  compoe: baseline + preset do harness + opcoes
      v
especificacao de lancamento (JSON versionado, por descritor de arquivo)
      |
      v
sandmux-run  --->  monta o isolamento  --->  executa o harness
```

O lançador só entende a especificação: lista de montagens, regras de Landlock, filtro seccomp, ambiente e comando. Baseline, presets e detecção de toolchains moram numa biblioteca usada pelo cliente e, depois, pelo daemon.

- **Por quê**: mantém o código que roda na fase crítica mínimo e sem lógica de produto, e dá ao daemon a mesma interface que a linha de comando usa. A especificação também é o que os testes de fuga exercitam.
- **Alternativa descartada**: o lançador conhecer harnesses e compor a política. Deixaria o binário mais crítico grande e mudando a cada harness novo.
- **Formato**: JSON com campo de versão, passado por descritor de arquivo herdado. É uma interface entre programas; o TOML fica para a configuração escrita pelo usuário.

### 2. Camadas de isolamento e ordem de aplicação

```
sandmux-run (supervisor, fora)        espera, repassa sinais e codigo de saida
   |
   +-- user ns + mount ns + pid ns + ipc ns + uts ns
        |
        +-- processo 1 (init minimo)   recolhe orfaos, repassa sinais
             |
             1. monta a nova raiz e troca de raiz (pivot_root)
             2. monta /proc novo, /dev minimo, /tmp e /run privados
             3. descarta capabilities, no_new_privs
             4. Landlock (arquivos + escopo de sockets abstratos e sinais)
             5. filtro seccomp
             6. executa o harness
```

- **O UID do usuário é mapeado para ele mesmo**, não para root. As capabilities existem só durante a montagem e são descartadas antes de executar o harness.
- **Mount namespace decide o que existe; Landlock decide o que pode ser feito com o que existe.** As duas camadas descrevem a mesma política; Landlock é a segunda barreira caso uma montagem saia errada.
- **Seccomp entra por último**, porque a montagem usa chamadas que o filtro depois nega.
- **Processo 1 próprio**: o hermes cria processos em segundo plano (gateway). Sem um init que recolha órfãos, eles viram zumbis; e quando o processo 1 termina, o kernel encerra tudo que restou no sandbox.
- **Alternativa descartada**: usar o `bubblewrap` como dependência externa. Resolveria a montagem, mas não aplica Landlock, exigiria um binário de terceiros instalado e tiraria o controle sobre a ordem das camadas.

### 3. Filtro seccomp: lista de negação, com erro em vez de encerrar

O filtro nega um conjunto de chamadas e devolve erro de permissão; o resto passa.

- Negadas: montagem e troca de raiz, `bpf`, `perf_event_open`, carga de módulos e `kexec`, chaveiro do kernel (`keyctl`, `add_key`, `request_key`), `userfaultfd`, `io_uring`, e os `ioctl` que injetam teclas no terminal.
- **Chaveiro do kernel negado** porque ele não é isolado por namespace e pode expor chaves da sessão do usuário.
- **`io_uring` negado** porque abre arquivos sem passar pelas chamadas que a futura interceptação de arquivos vai observar.
- **`ptrace` permitido**: depuradores são uso legítimo, e o pid namespace e o Landlock já limitam o alvo a processos do próprio sandbox.
- **User namespaces aninhados permitidos** (decisão do usuário): navegadores que se isolam funcionam. As montagens herdadas ficam travadas pelo kernel num namespace aninhado e o Landlock continua valendo. Ferramentas que precisam montar (podman sem root, bubblewrap) não funcionam, porque um processo sob Landlock não pode montar nada, em nenhum namespace. Contêineres chegam por outro caminho, num change posterior: um Docker sem root que roda fora do Landlock e entrega só o socket ao agente.
- **Por que lista de negação**: uma lista de permissão quebra harnesses a cada versão de runtime (Bun, Python, Node). Com a ameaça alvo excluindo exploit de kernel, reduzir a superfície ao máximo não compensa o atrito.

### 4. Visão do sistema de arquivos

```
/                       tmpfs novo (nada do host por padrao)
|-- usr, bin, lib*, opt    host, somente leitura
|-- etc                    host, somente leitura, com substituicoes
|-- proc, dev, tmp, run    novos e privados
|-- sys                    host, somente leitura
|-- home/<usuario>         HOME sintetico (leitura e escrita)
|    |-- <toolchains>      overlay: host por baixo, camada privada por cima
|    |-- <caches>          overlay: host por baixo, camada privada por cima
|    |-- <pasta de entrada>   host, somente leitura
|    `-- <pasta de trabalho, se estiver sob o HOME>
`-- <pasta de trabalho>    host, leitura e escrita, no caminho original
     `-- .git/hooks, .git/config    somente leitura
```

- **A pasta de trabalho é a pasta atual** (decisão do usuário): por padrão, a pasta de onde o `run` foi executado; uma opção indica outra. É a pasta real do host, por montagem direta, e não uma cópia nem uma camada: o que o agente cria aparece no host na hora, e o que já está nela aparece para o agente. O HOME é separado; a pasta de trabalho, não.
- **Caminhos originais preservados.** O HOME sintético é montado no caminho do HOME real e a pasta de trabalho no caminho absoluto dela. Configs e ferramentas com caminhos absolutos continuam funcionando.
- **`/etc` do host com substituições**, em vez de um `/etc` construído do zero. `passwd` e `group` são gerados só com o usuário do sandbox; arquivos sensíveis conhecidos (`shadow`, `sudoers`, chaves de host SSH) são cobertos. Construir do zero seria mais fechado, mas programas dependem de dezenas de arquivos ali, e o que só root lê no host continua ilegível porque o sandbox não tem root de verdade.
- **`/run` vazio** é o que tira do alcance os sockets de ssh-agent, D-Bus, Docker e Wayland. Sockets abstratos, que não são arquivos, são bloqueados pelo escopo do Landlock.
- **Opções `--ro` e `--rw`** acrescentam pastas à visão. É a forma de gerenciar pastas nesta fatia; o cadastro com interface vem depois.
- **Pasta de entrada** (decisão do usuário): uma pasta no diretório de dados do sandmux, no host, é montada em somente leitura num caminho fixo do HOME sintético, em todo sandbox. É como o usuário entrega ao agente um arquivo de fora da pasta de trabalho, como um screenshot ou um PDF: colar imagem direto no harness não funciona, porque o clipboard do desktop fica fora do sandbox. É somente leitura para não virar canal de escrita entre sandboxes. A pasta é uma só, então o que está nela fica visível para todos os agentes até ser apagado. Colar imagem pela TUI, gravando nessa pasta, é de um change posterior.
- **Credencial conhecida montada de propósito** (decisão do usuário): é assim que CLIs de cloud funcionam na v1, por exemplo `--ro ~/.kube`. Quando o caminho montado está numa lista de credenciais conhecidas (`~/.aws`, `~/.kube`, a configuração do `gcloud`, `~/.ssh`), o `run` avisa que a credencial real fica ao alcance do agente.
- **Pasta de trabalho perigosa** (decisão do usuário): quando a pasta de trabalho é o HOME, a raiz ou uma pasta que contém o HOME, o `run` recusa sempre e explica, sugerindo uma subpasta. O mesmo vale para `--ro` e `--rw` apontando para esses caminhos. Não existe opção de forçar: ela foi considerada e descartada pelo usuário, porque o HOME real e o HOME sintético disputariam o mesmo caminho e a exceção só complicaria o desenho.
- **`/dev` mínimo, mais a GPU** (decisão do usuário): além de `null`, `zero`, `urandom`, `tty` e do pty, entram os dispositivos de GPU que existirem na máquina (os nós de renderização e os do driver da NVIDIA), para que treino e inferência local funcionem sem configurar. Os nós que controlam o monitor ficam de fora. `/dev/kvm` só entra com opção explícita. Uma opção desliga a GPU.

### 5. Toolchains por lista conhecida

Cada entrada da lista descreve onde a toolchain mora, o que montar e o que excluir. Exemplo: `cargo` monta `~/.cargo` e cobre `credentials.toml`; `uv` monta só os binários `uv` e `uvx` de `~/.local/bin`, nunca a pasta inteira.

- **Por quê** (decisão do usuário): previsível e sem risco de montar uma pasta do `PATH` com conteúdo sensível desconhecido.
- **Pastas de toolchain recebem uma camada privada de escrita** (decisão 6), para que instalações globais funcionem. Arquivos avulsos, como os binários do `uv`, entram em somente leitura.
- **Arquivos de credencial são cobertos por cima do overlay**, por uma montagem que o agente não consegue remover nem renomear.
- **Custo aceito**: uma ferramenta fora da lista não aparece até ser acrescentada com `--ro`.
- O `PATH` dentro do sandbox é reconstruído a partir das entradas montadas.

### 6. Camada privada de escrita com overlayfs, para caches e toolchains

Para cada cache conhecido (npm, pip, registry do cargo, uv, bun, módulos Go) e para cada pasta de toolchain da decisão 5, o sandbox monta um overlay: a pasta do host por baixo, em somente leitura, e uma camada de escrita no diretório de dados do sandmux.

- **Por quê nos caches** (decisão do usuário): não baixa de novo os cerca de 10 GB já presentes, e um pacote adulterado pelo agente nunca chega ao cache que o usuário usa fora do sandbox.
- **Por quê nas toolchains** (decisão do usuário): com as toolchains em somente leitura, `cargo install`, `npm install -g`, `rustup component add` e `go install` falhavam, e isso é uso corriqueiro de quem desenvolve. Com a camada, a instalação funciona dentro do sandbox e a toolchain do host continua intacta.
- **O que o agente instala fica**: uma ferramenta instalada na camada continua lá nas execuções seguintes do mesmo harness. Apagar a camada devolve a toolchain ao estado do host.
- **Uma camada de escrita por harness** (decisão do usuário), com trava. O que uma execução baixa fica disponível para as seguintes, em qualquer pasta de trabalho. O overlayfs não admite duas montagens usando a mesma camada de escrita, e sem o daemon não há quem mantenha uma montagem compartilhada; por isso, nesta fatia, uma segunda execução simultânea do mesmo harness recebe uma camada temporária e um aviso. O compartilhamento entre execuções simultâneas chega com o daemon.
- **Alternativa de recuo**: se o overlay falhar na máquina, os caches viram caches próprios vazios e as toolchains entram em somente leitura, com aviso. Nunca compartilhar uma pasta do host com escrita.

### 7. HOME sintético e presets

- Um HOME sintético por harness, em `<dados do sandmux>/homes/<harness>/`, montado com leitura e escrita. Execuções simultâneas compartilham a mesma pasta por montagem direta.
- O código do harness entra em somente leitura por cima: `~/.opencode` para o opencode; `~/.hermes/hermes-agent` e o wrapper `~/.local/bin/hermes` para o hermes. A atualização automática de dentro do sandbox deixa de funcionar; atualizar é feito fora, pelo usuário, com o atualizador do próprio harness (decisão do usuário). O agente nunca altera o código do harness.
- O preset desliga a checagem de atualização: no opencode, pela variável `OPENCODE_DISABLE_AUTOUPDATE`; no hermes, pela chave `updates.check: false` do `config.yaml`, gravada ao criar o HOME sintético e ao importar.
- **Harness não instalado** (decisão do usuário): quando o preset não encontra o harness na máquina, o `run` recusa e mostra como instalar no host. O sandmux não instala harness.
- **Preset `shell`** (decisão do usuário): um shell confinado, sem harness, lançado como os outros (`sandmux run shell`), com HOME sintético próprio. Usa o shell do usuário (`$SHELL`) quando ele está no sistema, e o `bash` caso contrário, com um arquivo de configuração mínimo gerado ao criar o HOME sintético (prompt e `PATH`). O `.bashrc` e o `.zshrc` reais não são copiados: costumam exportar tokens e chamar coisas que não existem lá dentro. O preset não tem importação. Serve de terminal para o modal de comando que virá com a TUI e para inspecionar o que um agente enxerga.
- O preset do hermes nunca monta o `gateway.sock` do host. O hermes confinado sobe o próprio gateway dentro do HOME sintético.
- **Gitconfig mínimo** (decisão do usuário): ao criar o HOME sintético, o cliente lê `user.name` e `user.email` do git do usuário, fora do sandbox, e grava um gitconfig só com esses dois valores. Aliases, credential helpers e URLs com token não entram. Sem isso o commit do agente falha por falta de autor. O arquivo não é regravado depois, então ajustes feitos pelo usuário no HOME sintético permanecem.
- O ambiente é construído do zero: lista segura (`TERM`, `LANG`, `PATH` reconstruído, `HOME`, variáveis XDG apontando para o HOME sintético), mais as variáveis do preset, mais as de `--pass-env`.

### 8. Importação com troca de segredos

`sandmux import <harness>` roda fora do sandbox e faz uma cópia de mão única do HOME real para o HOME sintético, guiada por uma tabela por harness com três classes:

| Classe | Exemplos | Tratamento |
|---|---|---|
| Configuração e conhecimento | config, skills, plugins, memórias do hermes, MCPs cadastrados | Copiado |
| Histórico | banco de sessões, saídas de ferramenta, logs, caches | Não copiado |
| Arquivo com segredo | `auth.json`, `.env` do hermes | Copiado com os valores trocados por valores falsos |

- **Valores falsos têm formato reconhecível** e um manifesto registra quais segredos foram trocados e de onde vieram, sem os valores. O change do cofre usa esse manifesto para saber o que cadastrar.
- **Na dúvida, é segredo.** No `.env` e no `config.yaml` do hermes, chaves com nome de credencial ou valores com formato de token são trocados, e o comando lista o que trocou. Trocar a mais só exige recadastrar; trocar a menos vaza.
- **Reimportação só do que é novo** (decisão do usuário): rodar o `import` de novo copia o que ainda não existe no HOME sintético, nunca sobrescreve e lista o que está diferente entre os dois lados. Uma opção explícita força sobrescrever. A unidade é o arquivo: um skill novo entra sozinho; um MCP novo cadastrado num arquivo de configuração que já existe aparece só como diferença, e o usuário decide entre sobrescrever e editar à mão.
- **Oferecida na primeira execução** (decisão do usuário): quando o `sandmux run` não encontra HOME sintético para o harness e existe estado real para importar, pergunta uma vez. Aceitando, importa, mostra o que foi copiado e quais segredos foram trocados, e segue para o lançamento. Recusando, cria o HOME sintético vazio e não pergunta de novo. Sem terminal interativo, o `run` exige uma opção explícita dizendo se importa ou não, e recusa iniciar sem ela.

### 9. Falha fechada e diagnóstico

O lançador verifica, antes de montar qualquer coisa, user namespaces sem privilégio, a versão de Landlock com escopo de sockets abstratos e overlayfs em user namespace. Faltando qualquer um, não executa o harness. `sandmux doctor` mostra cada verificação e, quando a distro restringe user namespaces por AppArmor, explica a causa.

- **Por que exigir o escopo de sockets abstratos agora**: sem network namespace nesta fatia, ele é a única barreira contra sockets abstratos do host. Quando o isolamento de rede chegar, o requisito pode baixar.
- **Degradação só na camada de escrita**: o recuo da decisão 6 é a única degradação aceita, porque não reduz o confinamento.

### 10. Ponto de extensão reservado para liberar pastas depois

O Landlock não pode ser afrouxado depois de aplicado, mas o change de política de arquivos vai precisar liberar pastas com o agente rodando. A nova raiz já nasce com um diretório vazio, pré-autorizado no Landlock, onde montagens futuras podem ser anexadas de fora. Autorizar um diretório vazio em tmpfs não concede acesso a nada hoje.

- A premissa de que o Landlock aceita o que for montado depois sob um diretório autorizado precisa ser confirmada por teste neste change.

### 11. Verificação por testes de fuga

Um programa de sondagem roda dentro do sandbox e tenta: ler `~/.ssh` e outros caminhos do HOME real, conectar em sockets do host (por caminho e abstratos), enxergar e sinalizar processos do host, ler variáveis de ambiente do processo pai, usar as chamadas negadas, escrever em caminhos de somente leitura, alterar o cache e as toolchains do host, ler os arquivos de credencial cobertos, gravar na pasta de entrada, e criar um hook ou alterar o `.git/config` da pasta de trabalho, inclusive num worktree. Cada tentativa precisa falhar. Iniciar numa pasta de trabalho perigosa também precisa falhar. Um segundo conjunto sobe opencode, hermes e o preset `shell` reais e confere que iniciam, que um commit e uma instalação global funcionam lá dentro, e que um arquivo posto na pasta de entrada é lido.

### 12. Proteção do `.git` da pasta de trabalho

A pasta de trabalho é do agente, mas dois pontos do `.git` são executados ou obedecidos pelo git do usuário fora do sandbox: os hooks e a configuração (que aceita comandos em chaves como `core.fsmonitor`, `core.sshCommand` e nos filtros). Um agente manipulado que escreva ali ganha execução no host na próxima vez que o usuário usar o git.

- **`.git/hooks` e `.git/config` entram em somente leitura** (decisão do usuário), por uma montagem por cima da pasta de trabalho. O mesmo vale para o diretório git de cada submódulo em `.git/modules`.
- **Por que montagem e não Landlock**: o Landlock só concede acesso; não consegue retirar a escrita de um caminho dentro de uma pasta já liberada.
- **O que continua funcionando**: commit, branch, merge, rebase e stash, que escrevem em objetos, referências e índice.
- **O que deixa de funcionar** (custo aceito): criar ou instalar hooks, `git remote add`, `git config` local e gravar o upstream de uma branch. Configurações que o agente precise cabem no gitconfig do HOME sintético.
- **Alternativas descartadas**: `.git` inteiro em somente leitura, que impede o agente de fazer commit; e nenhuma proteção, que deixa a fuga aberta.
- **Worktree do git** (decisão do usuário): o `.git` de um worktree é um arquivo que aponta para o repositório principal, fora da pasta de trabalho. O cliente detecta o caso e monta o diretório git do repositório principal no caminho original, com a mesma proteção de hooks e config. O agente passa a alcançar o histórico e as branches dos outros worktrees do mesmo repositório.
- **Subpasta de um repositório** (decisão do usuário): quando o `.git` fica acima da pasta de trabalho, o padrão é subir sem git, com aviso. Uma opção explícita monta o repositório inteiro em somente leitura, com a subpasta gravável e o `.git` protegido. O agente só enxerga o resto do repositório se o usuário pedir. Montar só o `.git`, sem os outros arquivos, foi descartado: o git veria o resto do repositório como apagado, e um commit descuidado registraria essas remoções.
- **Limite**: se o `.git/config` do usuário já aponta `core.hooksPath` para uma pasta versionada (caso do husky), os hooks moram na pasta de trabalho e continuam graváveis. E um repositório criado pelo agente durante a execução nasce sem proteção até o próximo lançamento.

## Risks / Trade-offs

- [Rede do host aberta e sem registro nesta fatia] → Declarado em `proposal.md`; o conteúdo da pasta de trabalho pode sair. Mitigado só quando o change de rede chegar.
- [`--pass-env` coloca um segredo real ao alcance do agente] → Desligado por padrão, aviso a cada execução, e deixa de ser necessário para chaves quando o cofre existir.
- [Sem PTY intermediário, o agente escreve sequências de escape direto no terminal do usuário] → A injeção de teclas é negada no seccomp; o filtro de sequências fica para o daemon.
- [O cache do host muda enquanto o overlay está montado, o que o overlayfs não garante] → Medir na prática com npm e cargo; se houver corrupção, cair para cache próprio.
- [Kernel 6.12 exclui distros estáveis mais antigas] → Aceito nesta fatia; o requisito baixa com o isolamento de rede.
- [Lista de exclusão de credenciais em toolchains e em `/etc` nunca é completa] → Listas pequenas, revisáveis e versionadas com o binário; os testes de fuga cobrem cada entrada.
- [Segredo dentro do histórico do git da pasta de trabalho] → Fora do alcance desta fatia; o agente lê a pasta de trabalho inteira.
- [Arquivos de segredo que já estão na pasta de trabalho (`.env`, chaves) ficam legíveis, com a rede aberta] → Confirmado pelo usuário: nada é escondido antes do change de política de arquivos. Cobrir na hora do lançamento os arquivos existentes foi considerado e descartado. Quem usa esta fatia precisa tirar os segredos da pasta.
- [O que é posto na pasta de entrada fica visível para todos os agentes] → A pasta é somente leitura lá dentro e o usuário apaga o que não quer mais expor.
- [Arquivos da pasta de trabalho que o usuário executa no host: `Makefile`, scripts do `package.json`, `.envrc`, tarefas do editor, hooks versionados] → A decisão 12 cobre só o `.git`. O que o agente escreve precisa ser lido antes de ser executado fora do sandbox.
- [Ferramenta adulterada instalada pelo agente na camada das toolchains persiste entre execuções] → Fica restrita ao sandbox do mesmo harness; apagar a camada devolve o estado do host.
- [Proteção do `.git` quebra fluxos que gravam no `.git/config`, como criar uma branch que acompanha uma remota] → Custo aceito; medir com opencode e hermes reais quais operações do dia a dia esbarram nisso.
- [O agente não roda contêineres nem instala pacotes do sistema nesta fatia] → O Docker privado chega no change do daemon.
- [GPU liberada por padrão expõe os drivers de vídeo a todo agente] → Aceito por decisão do usuário; exploit de kernel está fora do escopo, e uma opção desliga.
- [Montar uma credencial de cloud entrega ao agente o que a baseline esconde] → Só por opção explícita, com aviso a cada execução.
- [Num worktree, o agente alcança as branches dos outros worktrees do mesmo repositório] → Aceito por decisão do usuário; é o mesmo repositório.
- [Heurística de segredo erra no `config.yaml` do hermes] → Regra "na dúvida, é segredo" e relatório do que foi trocado.
- [Harness tenta se atualizar e falha por código em somente leitura] → Os presets desligam a checagem de atualização (decisão 7); atualizar é feito no host.
- [O harness de dentro fica desatualizado se o usuário só o usa pelo sandmux] → Aceito por decisão do usuário; um comando do sandmux que roda o atualizador fora do sandbox ficou para depois da v1.
- [HOME sintético compartilhado permite que um projeto contamine skills ou memória usadas por outro] → Aceito por decisão do usuário.

## Open Questions

Nenhuma decisão em aberto. Resta um fato a confirmar por teste: que a chave `updates.check: false` no `config.yaml` do hermes desliga a checagem de atualização. Ela foi encontrada em 2026-11-12 no código do hermes instalado na máquina de referência (`hermes_cli/source_check.py`) e ainda não foi exercitada.

As listas iniciais (arquivos sensíveis cobertos em `/etc` e credenciais excluídas por toolchain) não são questão em aberto: por decisão do usuário, o conteúdo delas é escrito nas specs deste change para revisão item a item.
