# Roadmap do sandmux

Registro da exploração feita em 2026-11-12 e plano sequencial de desenvolvimento. Este arquivo é a memória do projeto enquanto as specs não existem: quando um change for proposto, os artefatos dele passam a valer sobre o que está escrito aqui, e este roadmap deve ser atualizado.

Como ler: a seção 2 guarda todas as decisões já tomadas; a seção 4 organiza o trabalho em etapas, cada uma com os changes necessários, em ordem; a seção 7 liga cada pedido original ao change que o atende.

## 1. Visão

O sandmux é um multiplexador de terminal no estilo do tmux, escrito em Rust com ratatui, feito para subir harnesses de agente (opencode, hermes e outros) dentro de um sandbox de segurança.

O foco principal é o sandbox, não a interface:

- o agente fica confinado a uma pasta de trabalho;
- arquivos, pastas, rede, APIs de inferência e MCPs são controlados por allowlist e denylist;
- toda interação com arquivos e rede é registrada;
- nenhum segredo real existe dentro do sandbox: chaves são trocadas no tráfego de saída e credenciais de banco são injetadas por um MCP próprio.

**Princípio condutor**: seguro por padrão, com o mínimo de atrito. Uma baseline segura vem embutida e funciona sem configurar nada; o usuário só gerencia exceções. Na dúvida, prefere-se o padrão que não quebra programas.

## 2. Decisões tomadas

### 2.1 Plataforma e binários

| Tema | Decisão |
|---|---|
| Sistema | Apenas Linux |
| Isolamento | seccomp e Landlock, mais namespaces (user, mount, network, pid) |
| Binários | Quatro, separados: cliente `sandmux` (linha de comando e TUI), daemon de sessões `sandmuxd`, lançador do sandbox `sandmux-run`, auxiliar root `sandmux-helper`. Substitui o pedido original de binário único |
| Persistência | Os agentes sobrevivem ao fechar o terminal, como no tmux |
| Instalação | Pede `sudo` logo de início. Se recusado ou indisponível, instala no modo sem privilégio |
| Sem suporte de kernel | Sem Landlock e user namespace, o agente não sobe. Nunca existe aba sem sandbox |
| eBPF | Fica no roadmap, como otimização. Liga sozinho quando distro e kernel suportam; sem suporte, vale a pilha sem privilégio |
| Regras do eBPF | O eBPF pode ter regras exclusivas. Cada uma é marcada como obrigatória (a aba não sobe sem eBPF) ou opcional (sobe com aviso) |
| Ameaça alvo | Agente manipulado por prompt injection, tentando ativamente ler segredos e exfiltrar dados. Exploit de kernel fora do escopo |
| Nome | `sandmux`, escolhido pelo usuário em 2026-11-12. Substitui `asmux`, escolhido antes no mesmo dia, e o nome de trabalho `agentmux`. Os binários seguem o nome: `sandmux`, `sandmuxd`, `sandmux-run`, `sandmux-helper` |
| Licença | MIT e Apache-2.0, licença dupla |
| Distribuição na v1 | Binário estático com script de instalação, `cargo install`, pacotes deb e rpm, AUR e Homebrew |
| Distros suportadas na v1 | Ubuntu LTS atual (26.04), Debian estável (13), Fedora atual e Arch: são as testadas e prometidas. O Ubuntu 24.04 com o kernel original (6.8) fica de fora pelo requisito de kernel do primeiro change |

### 2.2 Política e configuração

| Tema | Decisão |
|---|---|
| Camadas de política | Baseline embutida, preset do harness, ajustes globais, ajustes da aba ou perfil. Negar sempre vence |
| Arquivo de configuração | TOML. Guarda só a diferença em relação às camadas embutidas |
| Gerenciamento | Cada regra aparece como "padrão" ou "modificada"; desligar regra da baseline é ato explícito |
| Perfil de um projeto | Lido só de fora da pasta de trabalho: fica na configuração do usuário, ligado ao caminho da pasta. Nada dentro da pasta altera a política, porque o agente poderia editar o arquivo. Custo aceito: o perfil não é versionado no repositório |
| Toda aba tem sandbox | Não existe shell livre |
| Opções do `run` no primeiro change | `--workdir`, `--ro`, `--rw`, `--pass-env`, `--import`, `--no-import`, `--repo`, `--kvm`, `--no-gpu`, e `--` para repassar argumentos ao harness. Os nomes foram propostos na spec e ainda não foram revisados pelo usuário |
| Diretório de dados | `$XDG_DATA_HOME/sandmux` (por padrão `~/.local/share/sandmux`), com os HOMEs sintéticos, as camadas de escrita, a pasta de entrada e os manifestos de importação |
| Listas embutidas | O conteúdo inicial de cada lista (arquivos sensíveis de `/etc`, credenciais por toolchain, padrões de arquivo de segredo, catálogo de exfiltração, catálogo de domínios comuns) é proposto na spec do change que a usa, para revisão item a item. As do primeiro change estão na spec `sandbox-filesystem` (arquivos de `/etc`, toolchains e seus arquivos de credencial, caches, credenciais conhecidas para o aviso de montagem) e na `harness-state-import` (o que é copiado de cada harness) |
| Atualização dos catálogos | Só com versão nova do sandmux: os catálogos são versionados junto com o binário, e o sandmux nunca faz chamada de rede por conta própria |

### 2.3 Arquivos

| Tema | Decisão |
|---|---|
| Visão do agente | Pasta de trabalho, HOME sintético, sistema em somente leitura e toolchains lidas do host |
| Pasta de trabalho | Por padrão, a pasta de onde o `run` foi executado. É a pasta real do host, montada no mesmo caminho com leitura e escrita, e não uma cópia: o que o agente cria aparece no host na hora, e o que já está nela (um PDF, por exemplo) aparece para o agente. O HOME é separado; a pasta de trabalho, não. Confirmado pelo usuário em 2026-11-12 |
| `/etc` | Substituir em vez de negar: `passwd` e `group` gerados, arquivos sensíveis cobertos |
| HOME sintético | Um por harness, compartilhado entre abas |
| Toolchains do HOME real | Só lista conhecida (nvm, bun, cargo, rustup, uv, pyenv, go, linuxbrew), sem arquivos de credencial. O host nunca é alterado |
| Instalações globais | As toolchains são lidas do host e escritas numa camada privada por harness, o mesmo mecanismo dos caches. `cargo install`, `npm install -g`, `rustup component add` e `go install` funcionam dentro do sandbox. Substitui a decisão anterior de toolchains em somente leitura |
| Caches de pacotes | Lidos do host, escritos numa camada privada. Uma camada por harness, compartilhada entre abas e pastas de trabalho. A spec propõe também os caches de navegadores do Playwright e de modelos do Hugging Face, com os tokens do Hugging Face cobertos; os dois não estavam na lista original e aguardam revisão |
| Arquivos com cara de segredo na pasta de trabalho | Escondidos do agente. O agente pode criar arquivos assim e continua vendo os que ele mesmo criou. Chega junto com a interceptação de arquivos, na etapa 6. Esconder antes, já no primeiro change, os arquivos que existem na hora do lançamento foi considerado e descartado pelo usuário em 2026-11-12 |
| Denylist de arquivo | Efeito imediato no agente em execução |
| Liberar pasta nova | Vale na hora, sem reiniciar o agente |
| User namespaces aninhados | Permitidos. Navegadores headless que se isolam funcionam; ferramentas que precisam montar (podman sem root, bubblewrap) não, porque um processo sob Landlock não monta nada |
| `.git` da pasta de trabalho | `.git/hooks` e `.git/config` em somente leitura. O agente faz commit, branch e merge; não cria hook, não adiciona remote e não grava o upstream de uma branch. Impede que algo plantado pelo agente rode fora do sandbox quando o usuário usa o git no host. A brecha do `.git/commondir` criado pelo agente fica para a etapa 6 (decisão de 2026-11-13) |
| Pasta de trabalho perigosa | Quando é o HOME, a raiz ou uma pasta que contém o HOME, o `run` recusa sempre e explica. Não existe opção de forçar: o usuário voltou atrás dela em 2026-11-12, por ser mais simples. Na implementação foram acrescentados o diretório de dados do sandmux e qualquer pasta que o contém (como `~/.local`), que dariam ao agente acesso às camadas e aos HOMEs sintéticos dos outros harnesses |
| Pasta de trabalho que é um worktree do git | O sandmux detecta e monta o diretório git do repositório principal, com hooks e config em somente leitura. O agente alcança o histórico e as branches dos outros worktrees |
| Pasta de trabalho que é subpasta de um repositório | Por padrão sobe sem git, com aviso. Uma opção explícita monta o repositório inteiro em somente leitura, com a subpasta gravável e o `.git` protegido |
| Dispositivos | A GPU entra em todo sandbox por padrão, quando existe na máquina. `/dev/kvm` só por opção explícita. O resto do `/dev` continua mínimo |
| Identidade do git | O HOME sintético recebe um gitconfig mínimo, gerado só com `user.name` e `user.email` copiados do gitconfig real. Aliases, credential helpers e URLs com token ficam de fora |
| Pasta de entrada | Uma pasta do sandmux no host, visível em somente leitura em todo sandbox, num caminho fixo (`~/inbox`, informado pela variável `SANDMUX_INBOX`). É o caminho para entregar ao agente um arquivo de fora da pasta de trabalho (screenshot, PDF). Na TUI, colar uma imagem grava nela e escreve o caminho no terminal |

### 2.4 Rede

| Tema | Decisão |
|---|---|
| Modos por aba | Quatro: **aberta** (padrão), **só inferência**, **sem rede**, **custom** |
| Modo aberta | Tudo liberado e registrado; bloqueio pela denylist, com efeito imediato |
| Denylist do modo aberta | Já vem com catálogo de destinos de exfiltração conhecidos, habilitado; itens podem ser desligados |
| Modo custom | Bloqueado por padrão. Allowlist com itens habilitados ou desabilitados. Catálogo de domínios comuns (domínios individuais, com curingas) que vem desabilitado. Domínios próprios podem ser cadastrados, habilitados ou não |
| Liberar no modo custom | Um domínio bloqueado pode ser liberado direto da lista de bloqueios do painel, do mesmo jeito que um acesso novo vai para a denylist. A liberação entra na allowlist da aba ou do perfil e vale na hora |
| Rede local | No modo aberta, inclui a rede local e os serviços da máquina. Metadados de cloud sempre bloqueados |
| Serviços da máquina | Alcançados por um nome especial, informado ao agente |
| Portas abertas pelo agente | Publicadas automaticamente no `localhost` do host e listadas no painel |
| Decifração de TLS | Só nos domínios com segredo a injetar; ampliável por perfil |
| Log de rede | Domínio, método e caminho, sem corpo. Onde não há decifração, domínio e volume |
| Retenção do log | 30 dias com teto de tamanho, ajustáveis |
| QUIC (HTTP/3) | Bloqueado. Os programas caem para TLS comum, onde o domínio aparece e a troca de chave funciona |
| Outros protocolos TCP e UDP | No modo aberta, passam e são registrados por domínio, IP e porta. Nos modos custom e só inferência, só passa o que casa com a allowlist |
| Conexão direta por IP, sem consulta de DNS | No modo aberta, passa, registrada e destacada como "sem domínio" |
| Ping | Não funciona dentro do sandbox |
| Aviso de domínio novo ou de bloqueio | Destaque no painel lateral e marca na aba, visível de outra aba. Nada fora do sandmux |
| Interface de rede | Criada por sandbox, sem privilégio; não precisa de interface criada na instalação |

### 2.5 Segredos

| Tema | Decisão |
|---|---|
| Regra | Nenhum segredo real dentro do sandbox |
| Cofre | Arquivo cifrado com senha, digitada quando o daemon sobe. Keyring no roadmap |
| Após reboot | O daemon volta sozinho no login do usuário; os agentes esperam a senha do cofre |
| Troca de chave | Chave falsa no sandbox, trocada pela real no tráfego, só quando o destino é o dono da chave |
| Provedor da v1 | OpenRouter, com API key |
| OAuth | Desejado; fica no roadmap |
| Git | ssh-agent controlado: chaves escolhidas por perfil, uso livre com registro, SSH de saída restrito aos servidores git cadastrados |
| Tokens de ferramentas de desenvolvimento | Segredo genérico por domínio, na v1: o cofre aceita qualquer token ligado a um domínio (GitHub para o `gh`, registries privados de npm, pip e cargo). O agente usa um valor falso e a troca acontece no tráfego, como na inferência |
| Até o cofre existir | `--pass-env`, desligado por padrão e com aviso, passa uma variável específica para dentro. No opencode, `OPENCODE_CONFIG_CONTENT` faz a variável vencer o `auth.json` importado. No hermes (decisão do usuário, 2026-11-13), o `~/.hermes/.env` daquela execução é um arquivo gerado, em somente leitura, com os valores reais no lugar dos falsos; o `.env` guardado continua falso. Valores com quebra de linha ou `${` são recusados |

### 2.6 Harnesses

| Tema | Decisão |
|---|---|
| v1 | opencode e hermes |
| Depois | Claude Code e oh-my-pi |
| Hermes | Completo: gateway próprio, cron e canais de mensagem. Nunca usa o gateway do host |
| Canais do hermes na v1 | Telegram, Discord e Slack. WhatsApp fica para depois |
| Backend Docker do hermes | Não suportado dentro do sandbox |
| Importação | Config, skills, memórias do hermes e MCPs cadastrados. Sem sessões nem saídas de ferramenta. Segredos trocados por valores falsos, com o prefixo `sandmux-fake-`. A tabela de cada harness é uma lista de permissão: o que não está nela não é copiado. Pareamento de canais e tarefas agendadas do hermes ficam de fora até a etapa 8 |
| Primeira execução de um harness | Com o HOME sintético vazio, o `run` pergunta uma vez se deve importar a configuração existente, mostra o que foi copiado e quais segredos foram trocados, e segue. Em uso não interativo, exige uma opção explícita |
| Reimportação | Rodar `sandmux import` de novo copia só o que ainda não existe no HOME sintético, nunca sobrescreve e mostra o que está diferente. Uma opção força sobrescrever |
| Preset `shell` | Um shell confinado, sem harness, lançado como qualquer harness (`sandmux run shell`), com HOME sintético próprio. Usa o shell do usuário (`$SHELL`) com um arquivo de configuração mínimo gerado pelo sandmux; o `.bashrc` e o `.zshrc` reais não são importados |
| Harness não instalado | O `run` recusa e mostra como instalar o harness no host. O sandmux não instala harness |
| Atualização do harness | Feita fora do sandbox, pelo usuário, com o atualizador do próprio harness. Dentro, o código entra em somente leitura e o preset desliga a checagem de atualização: no opencode pela variável `OPENCODE_DISABLE_AUTOUPDATE`; no hermes pela chave `updates.check: false` do `config.yaml` |
| MCPs importados | Liberados e registrados; o usuário desabilita os que não quiser |
| Controle de MCPs | Por servidor: cada servidor MCP é habilitado ou desabilitado inteiro. Sem controle por ferramenta |
| MCP de banco | PostgreSQL e MySQL. Leitura e escrita por padrão, com registro de cada query |

### 2.7 Interface

| Tema | Decisão |
|---|---|
| Layout | Abas, mais um painel lateral fixo para o registro de acessos. Sem painéis divididos genéricos |
| Atalhos | Tecla de prefixo, como no tmux, configurável. O padrão é Ctrl+b |
| Sessões | Um conjunto único de abas por usuário: todo cliente que se anexa vê as mesmas abas. No daemon, cada aba é uma sessão com o seu PTY. Sessões nomeadas, cada uma com as suas abas, ficam para depois da v1 |
| Vários clientes | Vários terminais podem se anexar ao mesmo tempo e todos interagem. O tamanho da tela segue o menor terminal |
| Aba de harness encerrado | Continua aberta, com o código de saída e o histórico; uma tecla reinicia o harness na mesma pasta e outra fecha a aba |
| Histórico de tela | 10.000 linhas por aba, em memória, ajustável. Some quando o daemon reinicia; nada da saída do agente vai para o disco |
| Escrita no clipboard pelo harness (OSC 52) | Bloqueada por padrão; uma opção por perfil ou aba libera. Selecionar e copiar pela TUI do sandmux funciona sempre |
| Início do daemon | No login do usuário. Subir no boot, sem login, fica para depois da v1, junto com o keyring |
| Programas com janela | Não abrem na v1: Wayland e X11 ficam fora do sandbox. Navegador headless funciona |
| Recursos | Teto generoso de processos e memória por aba, ajustável |
| Pedidos originais | Atalhos para navegar entre abas, modal com lista de abas, clique do mouse, nome de aba customizável, temas para abas, componentes, janelas e botões |
| Assistente | Modal de chat que abre uma aba de conversa; modal que gera um comando shell e o injeta no terminal da aba |
| Comando gerado | Só é escrito na linha do terminal; o usuário lê, edita se quiser e executa |
| Contexto do modal de comando | Vão ao modelo só a pergunta, a pasta atual e o shell em uso. Nada da tela da aba é enviado |
| Modelo do assistente | Um modelo padrão para chat e outro para comandos na configuração; o modal mostra qual está em uso e permite trocar |

### 2.8 Ferramentas de desenvolvimento

Pedido de 2026-11-12: os usuários serão desenvolvedores e precisam do que desenvolvedores usam no dia a dia, como o Docker, sem abrir mão do confinamento.

| Tema | Decisão |
|---|---|
| Socket do Docker do host | Nunca entra no sandbox. Ele equivale a root na máquina |
| Docker para o agente | Um Docker sem root, privado do sandbox, iniciado pelo sandmux fora do Landlock. O agente recebe só o socket dele. `docker` e `docker compose` funcionam; montagens só alcançam o que o sandbox já enxerga; `--privileged` não dá privilégio real |
| Rede dos contêineres | Passa pelo mesmo gateway, com o mesmo registro e a mesma política da aba, quando a etapa de rede chegar. Até lá fica tão aberta quanto a do sandbox |
| Imagens e contêineres | Separados dos do host. O agente não vê nem altera os contêineres que o usuário roda fora |
| Divisão do Docker privado | Um por pasta de trabalho. Imagens e volumes persistem entre sessões do mesmo projeto; os contêineres só enxergam aquele projeto e seguem a política de rede da aba. Um cache local de downloads evita baixar a mesma imagem de novo em outro projeto |
| Duas abas na mesma pasta | Dividem o mesmo Docker privado. Com modos de rede diferentes, vale para os contêineres o mais restrito |
| Máquina sem Docker rootless | O agente sobe sem Docker; `docker` lá dentro falha com mensagem clara e o diagnóstico diz o que instalar. Na v1 o único motor suportado é o Docker rootless |
| CLIs de cloud (`aws`, `gcloud`, `kubectl`, `terraform`) | Na v1, liberação explícita: o usuário monta a credencial de propósito (`--ro ~/.kube`), com aviso de que ela fica ao alcance do agente. Depois da v1, credencial temporária gerada fora do sandbox |
| Quando | Etapa 2, com o daemon, que mantém o Docker privado vivo junto com a sessão |
| Instalações globais | Camada privada de escrita sobre as toolchains (seção 2.3) |
| `gh` e registries privados | Segredo genérico por domínio (seção 2.5) |

## 3. Arquitetura

### 3.1 Binários

```
+----------------+  socket  +----------------------+  socket  +----------------+
| sandmux        |<-------->| sandmuxd             |<-------->| sandmux-helper |
| cliente        |          | daemon de sessoes    |          | auxiliar root  |
| (CLI e TUI)    |          | PTYs, gateway, cofre |          | eBPF           |
+----------------+          | registro, politica   |          | (roadmap)      |
                            +----------+-----------+          +----------------+
                                       |
                                       | especificacao de lancamento
                                       v
                            +----------------------+
                            | sandmux-run          |
                            | lancador do sandbox  |
                            +----------+-----------+
                                       |
                                       v
                                    harness
```

Fronteiras de confiança: o cliente não toca em segredo nem em sandbox; o daemon não tem código de interface; o lançador só aplica uma especificação pronta; o auxiliar root não tem rede, TLS nem banco.

### 3.2 Camadas do sandbox

```
  agente (opencode, hermes)
     |
     |  chamadas de sistema            trafego de rede
     v                                    v
+----------------------------+   +---------------------------+
| seccomp: nega chamadas     |   | network namespace:        |
| perigosas; interceptacao   |   | sem rota de saida, tudo   |
| de aberturas de arquivo    |   | passa pelo daemon         |
+----------------------------+   +-------------+-------------+
| Landlock: o que pode ser   |                 |
| feito com o que existe     |                 v
+----------------------------+   +---------------------------+
| mount namespace: o que     |   | gateway: politica de      |
| existe para o agente       |   | dominio, decifracao, troca|
+----------------------------+   | de chave, registro        |
| user e pid namespaces      |   +-------------+-------------+
+----------------------------+                 |
                                               v
                                  internet e rede local
```

### 3.3 Níveis de proteção

```
a cada inicio do daemon: detecta o suporte da maquina
      |
      +-- tem eBPF --------> nivel 2: observacao e regras no kernel   (roadmap)
      +-- nao tem ---------> nivel 1: seccomp + Landlock + namespaces (v1)
      +-- nem isso --------> recusa subir o agente
```

O nível ativo é sempre visível na interface e no diagnóstico.

## 4. Etapas e changes

Cada etapa é desenvolvida em sequência. Dentro de uma etapa, cada change passa por proposta, validação, implementação e arquivamento antes do seguinte. Nomes de change são provisórios até a proposta. O número é só um identificador: a ordem de desenvolvimento é a das tabelas.

Situação: **feito**, **em planejamento** (artefatos parciais) ou **a propor**.

A ordem das etapas foi confirmada: daemon e TUI vêm antes de rede e cofre. O cofre precisa do daemon para pedir a senha, o bloqueio imediato precisa de um processo de longa duração, e cada etapa a partir da 3 entrega junto a sua tela de gerenciamento.

### Etapa 1 - Confinamento local

Objetivo: opencode e hermes rodando confinados por linha de comando, sem daemon nem TUI. Ataca a maior incerteza do projeto.

| # | Change | Situação | Conteúdo |
|---|---|---|---|
| 1 | `add-sandbox-cli` | Em implementação: 57 de 60 tarefas feitas até 2026-11-13, com 123 testes passando (inclusive a suíte de fuga, 51 tentativas negadas). Faltam 2.1 (perfil AppArmor com a restrição ligada, exige `sudo`), 8.4 e 11.2 (hermes e opencode com a chave real do modelo). Nenhuma decisão em aberto | Lançador `sandmux-run`; cliente `sandmux` com `run`, `import` e `doctor`; baseline embutida; HOME sintético; `/etc` substituído; toolchains por lista conhecida, com camada privada de escrita; camada privada de cache por harness; presets de opencode, de hermes e de `shell` (shell confinado, com configuração mínima); erro com instrução quando o harness não está instalado; importação com troca de segredos, oferecida na primeira execução, e reimportação só do que é novo; pasta de entrada em somente leitura; `.git/hooks` e `.git/config` em somente leitura; suporte a worktree e opção para subpasta de repositório; recusa de pasta de trabalho perigosa; GPU por padrão e KVM por opção; aviso ao montar credencial conhecida; gitconfig mínimo com nome e e-mail; falha fechada; `--pass-env`; perfil AppArmor do lançador no repositório, com instalação manual mostrada pelo `doctor`; suíte de testes de fuga |

Limitações aceitas nesta etapa: rede do host aberta e sem registro; sem filtro de sequências de escape do terminal; exige kernel 6.12 ou mais novo; no Ubuntu, o perfil AppArmor do lançador é instalado à mão, com `sudo`.

Verificações: camada de cache com overlayfs em user namespace; premissa do diretório pré-autorizado no Landlock para liberar pastas depois; o que de fato funciona com user namespace aninhado; opencode e hermes reais iniciando.

### Etapa 2 - Daemon de sessões

Objetivo: agentes que sobrevivem ao fechar o terminal, e um processo de longa duração que passa a hospedar política, registro, gateway e cofre nas etapas seguintes.

| # | Change | Situação | Conteúdo |
|---|---|---|---|
| 2 | `add-session-daemon` | A propor | Binário `sandmuxd`; um PTY por sessão, com o daemon no meio; lança o harness pelo `sandmux-run`; sessões sobrevivem ao cliente; um conjunto único de sessões por usuário, sem sessões nomeadas; sessão encerrada fica registrada com o código de saída e pode ser reiniciada; comandos para criar, listar, anexar, desanexar e encerrar; vários clientes anexados à mesma sessão, todos interagindo; histórico de 10.000 linhas por sessão em memória, ajustável; protocolo versionado entre cliente e daemon, com regra para versões diferentes após atualização; filtro de sequências de escape (escrita no clipboard bloqueada por padrão, liberável por opção); início automático no login e restauração das sessões após reboot |
| 3 | `add-config-profiles` | A propor | Arquivo TOML; camadas de política (baseline, preset, global, perfil ou aba); só a diferença é gravada; perfis de sandbox; perfil de projeto ligado ao caminho da pasta, nunca lido de dentro dela; liberação de dispositivos e desligamento da GPU por perfil; liberação da escrita no clipboard por perfil ou aba; persistência das abas e de suas configurações; marcação de regra "padrão" ou "modificada" |
| 4 | `add-resource-limits` | A propor | Teto generoso de processos e memória por sessão, ajustável; medição de consumo por sessão, incluindo os contêineres do Docker privado |
| 24 | `add-container-engine` | A propor | Docker sem root, privado do sandbox, iniciado e mantido pelo daemon junto com a sessão; visão de arquivos igual à do sandbox; só o socket dele entra no sandbox; `docker` e `docker compose` funcionando; imagens e contêineres separados dos do host; um Docker privado por pasta de trabalho, com imagens e volumes que persistem; cache local de downloads de imagem; abas na mesma pasta dividem o Docker, com a política de rede mais restrita; sem Docker rootless na máquina, o agente sobe sem Docker e o diagnóstico explica |

Verificações: limites de recurso sem privilégio na máquina de referência; Docker rootless iniciado pelo sandmux com a visão de arquivos do sandbox (a máquina de referência tem `dockerd-rootless.sh`, `rootlesskit` e `newuidmap`, mas não tem `slirp4netns` nem `pasta`, que a rede do rootless costuma exigir).

### Etapa 3 - Multiplexador

Objetivo: a interface de terminal. A partir daqui, cada etapa entrega também a tela de gerenciamento do que constrói.

| # | Change | Situação | Conteúdo |
|---|---|---|---|
| 5 | `add-tui-tabs` | A propor | Cliente em ratatui; abas, cada uma com um terminal confinado; criar aba escolhendo harness ou shell e a pasta de trabalho; aba de harness encerrado continua aberta, com teclas para reiniciar e fechar; colar imagem grava na pasta de entrada e escreve o caminho no terminal; emulação de terminal; tecla de prefixo (Ctrl+b por padrão) e atalhos configuráveis para navegar entre abas; modal com a lista de abas; renomear abas; mouse (clique nas abas, repasse ao harness, seleção e cópia, rolagem) |
| 6 | `add-tui-themes` | A propor | Temas de cores para abas, componentes, janelas e botões, definidos no TOML |
| 7 | `add-side-panel` | A propor | Painel lateral fixo por aba: nível de proteção ativo, consumo de recursos e, nas etapas seguintes, registro de acessos, portas e MCPs |

Verificações: fidelidade do emulador de terminal com as TUIs do opencode e do hermes (tela alternativa, mouse, colagem, protocolo de teclado, cor verdadeira).

### Etapa 4 - Rede controlada

Objetivo: todo o tráfego do agente passa pelo daemon, é registrado e pode ser bloqueado.

| # | Change | Situação | Conteúdo |
|---|---|---|---|
| 8 | `add-network-gateway` | A propor | Network namespace por sandbox; interface criada por sandbox, sem privilégio; gateway no daemon; DNS respondido pelo daemon; modos **aberta** e **sem rede**; rede local e serviços da máquina alcançáveis no modo aberta; nome especial para a máquina; metadados de cloud bloqueados; registro de rede (domínio e volume); QUIC bloqueado; demais protocolos TCP e UDP registrados por domínio, IP e porta; conexão direta por IP destacada como "sem domínio"; sem ping; retenção de 30 dias com teto; registro no painel com destaque para domínio novo. Permite baixar o requisito de kernel da etapa 1. O tráfego do Docker privado passa pelo mesmo gateway, com a política da aba |
| 9 | `add-network-policy` | A propor | Denylist com efeito imediato; adicionar à denylist direto da lista de acessos novos; catálogo de destinos de exfiltração habilitado, versionado com o binário; marca na aba em domínio novo ou bloqueio; modos **só inferência** e **custom**; allowlist com itens habilitados ou desabilitados; catálogo de domínios comuns desabilitado; cadastro de domínios próprios; liberar um domínio direto da lista de bloqueios; tela de gerenciamento |
| 10 | `add-port-publishing` | A propor | Portas que o agente abre publicadas automaticamente no `localhost` do host, listadas no painel, inclusive as publicadas por contêineres do Docker privado |

### Etapa 5 - Segredos fora do sandbox

Objetivo: o harness chama o modelo e usa o git sem que nenhuma credencial real entre no sandbox. Encerra a necessidade de `--pass-env` para chaves.

| # | Change | Situação | Conteúdo |
|---|---|---|---|
| 11 | `add-secret-vault` | A propor | Cofre em arquivo cifrado com senha; desbloqueio quando o daemon sobe; sessões em "aguardando cofre" após reboot; backend trocável (keyring depois); cadastro de segredos; leitura do manifesto gerado pela importação |
| 12 | `add-tls-interception` | A propor | CA local; confiança injetada no sandbox; decifração só nos domínios com segredo, ampliável por perfil; registro de método e caminho no tráfego decifrado |
| 13 | `add-inference-key-injection` | A propor | OpenRouter; chave falsa trocada pela real só para o destino dono da chave; controle de quais APIs de inferência estão liberadas; modo **só inferência** derivado dos provedores cadastrados |
| 25 | `add-domain-secret-injection` | A propor | Segredo genérico por domínio: qualquer token cadastrado no cofre e ligado a um domínio é trocado no tráfego; valor falso gravado no arquivo de configuração da ferramenta; cobre o `gh`, o git por HTTPS e registries privados de npm, pip, cargo e de imagens de contêiner |
| 14 | `add-git-ssh-agent` | A propor | ssh-agent do sandmux dentro do sandbox; chaves escolhidas por perfil; registro de cada uso; SSH de saída restrito aos servidores git cadastrados |

Verificações: o runtime do opencode aceita a CA local.

### Etapa 6 - Política e registro de arquivos

Objetivo: registro de todo acesso a arquivo e política de arquivos que muda com o agente em execução.

| # | Change | Situação | Conteúdo |
|---|---|---|---|
| 15 | `add-file-access-log` | A propor | O daemon intercepta as aberturas de arquivo; registro de todo acesso; backend trocável (eBPF depois); retenção |
| 16 | `add-file-policy` | A propor | Allowlist e denylist de arquivos e pastas; negar com efeito imediato; liberar pasta nova na hora; arquivos com cara de segredo escondidos, com registro dos criados pelo próprio agente; lista de acessos novos com ação de bloquear; tela de cadastro e gerenciamento de pastas; negar a criação de `.git/commondir` na pasta de trabalho (brecha da proteção do `.git` achada no change 1) |

Verificações: custo da interceptação com opencode e hermes reais (se pesar demais, o recuo é aplicar a denylist no próximo início); montagem de pasta num sandbox em execução.

### Etapa 7 - MCP e banco de dados

Objetivo: controlar quais MCPs o agente usa e dar acesso a banco sem expor credencial.

| # | Change | Situação | Conteúdo |
|---|---|---|---|
| 17 | `add-mcp-control` | A propor | Allowlist e denylist de MCPs, por servidor; MCPs importados liberados e registrados; lista no painel com habilitar e desabilitar; MCPs remotos controlados pela rede; MCPs locais rodando dentro do sandbox |
| 18 | `add-database-mcp` | A propor | MCP embutido para PostgreSQL e MySQL; cadastro de conexões; usuário e senha no cofre, injetados fora do sandbox; leitura e escrita por padrão; registro de cada query |

### Etapa 8 - Hermes completo

Objetivo: gateway, cron e canais de mensagem do hermes funcionando confinados.

| # | Change | Situação | Conteúdo |
|---|---|---|---|
| 19 | `add-inbound-ports` | A propor | Publicação de portas de entrada para os servidores do hermes (API e webhooks), com allowlist e registro |
| 20 | `add-hermes-channels` | A propor | Troca de token por canal: Telegram (na URL), Discord (em mensagem WebSocket) e Slack (em header); cron e gateway de longa duração na sessão; espera pelo cofre após reboot |

### Etapa 9 - Assistente

Objetivo: usar os modelos cadastrados direto do multiplexador.

| # | Change | Situação | Conteúdo |
|---|---|---|---|
| 21 | `add-chat-tab` | A propor | Modal para escrever um texto e enviar com Enter a um modelo com chave cadastrada; a resposta abre numa aba de chat; conversa contínua, com barra de texto e botão de envio; modelo padrão vindo da configuração, com troca no modal |
| 22 | `add-shell-command-modal` | A propor | Modal para perguntar algo; o modelo devolve um comando shell; botão ou atalho escreve o comando no terminal da aba que está sob o modal, sem executar; ao modelo vão só a pergunta, a pasta atual e o shell em uso; modelo padrão vindo da configuração, com troca no modal |

### Etapa 10 - Instalação e distribuição

Objetivo: instalar em qualquer máquina suportada com um passo só.

| # | Change | Situação | Conteúdo |
|---|---|---|---|
| 23 | `add-installer` | A propor | Instalador que pede `sudo` e segue sem privilégio se recusado; perfil AppArmor em distros que restringem user namespace; instalação dos binários e do início automático do daemon; detecção do nível de proteção a cada início; diagnóstico; canais: binário estático com script, `cargo install` com comando de configuração à parte, pacotes deb e rpm, AUR e Homebrew; distros testadas e suportadas: Ubuntu 26.04, Debian 13, Fedora atual e Arch; licença MIT e Apache-2.0 |

Pode ser antecipada se for preciso testar em distros que restringem user namespace.

## 5. Depois da v1

Sem ordem definida. Cada item vira um ou mais changes quando for priorizado.

| Item | Conteúdo |
|---|---|
| Backend eBPF | Binário `sandmux-helper` como serviço root; observação de arquivos e rede no kernel; regras exclusivas marcadas como obrigatórias ou opcionais; ativação automática com recuo para o nível sem privilégio |
| Keyring | Keyring do sistema como backend do cofre; permite desbloqueio sem digitar a senha |
| OAuth e mais provedores | Login por assinatura, com reescrita da resposta de login e encaminhamento da porta de retorno; provedores além do OpenRouter |
| Validação de prompt injection | Inspeção do conteúdo que vem da internet e do input do usuário; depende de ampliar a decifração de TLS |
| Mais harnesses | Claude Code e oh-my-pi |
| WhatsApp no hermes | Quando houver forma de manter a sessão pareada fora do sandbox |
| Credencial temporária de cloud | O sandmux gera fora do sandbox uma credencial de curta duração e limitada para AWS, GCP e Kubernetes, e entrega só ela ao agente |
| Outros motores de contêiner | podman como alternativa ao Docker rootless; instalação do motor pelo próprio sandmux |
| Perfil versionado no repositório | Arquivo no projeto que só restringe a política, ou que afrouxa mediante aprovação fora do sandbox |
| Notificação do desktop | Aviso do sistema em domínio novo ou bloqueio, para quando o sandmux não está em foco |
| Catálogos por download | Atualização de catálogos assinados sem esperar versão nova, por comando ou automática |
| Histórico de tela em disco | Guardar a saída das abas em arquivo, para sobreviver ao reinício do daemon |
| Sessões nomeadas | Várias sessões, cada uma com as suas abas, como no tmux |
| Programas com janela | Opção explícita que libera o socket do Wayland, com aviso. O X11 nunca entra, porque permite capturar o teclado do desktop inteiro |
| Entrar no sandbox de uma aba | Abrir um shell dentro do sandbox de uma aba em execução, para ver exatamente o que o agente vê |
| Tela da aba como contexto do assistente | No modal de comando, um atalho inclui as últimas linhas do terminal na pergunta enviada ao modelo |
| Daemon no boot | Início sem login (linger), junto com o keyring, que é quando passa a fazer diferença |
| Atualização do harness pelo sandmux | Comando que roda o atualizador oficial do harness fora do sandbox |
| Configuração de shell importada | `.bashrc`, `.zshrc` e afins copiados para o preset `shell`, com troca de segredos |
| Importação sincronizada | Trazer o que é novo a cada execução, sem rodar o `import` |

## 6. Riscos e pontos em aberto

### Verificações que dependem de medição

| Verificação | Etapa |
|---|---|
| Camada de cache com overlayfs em user namespace. **Confirmado em 2026-11-12** (tarefa 2.2): overlay montado em user namespace sobre `~/.cargo`, `~/.nvm` e `~/.npm`, com a camada de escrita no HOME real, sem a opção `userxattr`; o host ficou idêntico | 1 |
| Montagem em user namespace com a restrição do AppArmor ligada, com e sem o perfil do lançador. O Ubuntu 26.04 traz `kernel.apparmor_restrict_unprivileged_userns=1` por padrão, mas a máquina de referência tem uma sobreposição local que põe 0 (corrigido em 2026-11-12: o registro anterior dizia que a máquina restringia). A prova exige religar a restrição temporariamente, com `sudo` | 1 |
| O opencode usa a chave de `OPENROUTER_API_KEY` quando o `auth.json` importado tem um valor falso? **Não** (tarefa 2.6, 2026-11-12, com servidor local): o `auth.json` vence a variável. Com `OPENCODE_CONFIG_CONTENT` definindo `apiKey` como `{env:OPENROUTER_API_KEY}` para o provedor, a variável vence. O preset do opencode usa esse mecanismo quando o `--pass-env` passa a chave de um provedor | 1 |
| Um arquivo coberto por montagem em cima do overlay não pode ser apagado nem renomeado de dentro do sandbox. **Confirmado em 2026-11-12** (tarefa 2.3): apagar, renomear e substituir por rename falham com EBUSY, escrever falha com EROFS, e renomear a pasta que é ponto de montagem falha com EBUSY | 1 |
| Diretório pré-autorizado no Landlock aceita montagens posteriores. **Confirmado em 2026-11-12** (tarefa 2.4): um processo restrito leu uma pasta montada sob o diretório autorizado depois da restrição, e continuou sem acesso fora dele | 1 |
| Cache do host alterado com o overlay montado. **Sem corrupção em 2026-11-12** (tarefa 2.7, em pequena escala, com um registro do cargo e um cache do npm de teste): o host baixou pacotes enquanto o sandbox compilava e gravava no cache; `npm cache verify` e builds offline passaram dos dois lados, e o sandbox enxergou os pacotes baixados pelo host durante a execução | 1 |
| O hermes carrega o `.env` do HOME sintético por cima do ambiente, e o escopo de credenciais dele lê as chaves direto do arquivo (achado de 2026-11-12, corrigido em 2026-11-13: a limpeza de chaves ausentes do `.env` só atinge variáveis de roteamento do próprio hermes). Com isso, o valor falso da importação vencia o `--pass-env`. **Resolvido em 2026-11-13** (tarefa 8.6): com `--pass-env`, o `.env` da execução tem o valor real; o `hermes status --full` real passou de `sand...` para o final da chave de teste passada, e o `.env` guardado continuou falso. O parser do hermes expande `${...}` mesmo entre aspas, por isso esses valores são recusados | 1 |
| Quem configura `prefix` no `~/.npmrc` (por exemplo `~/.npm-global`) tem as ferramentas globais do npm fora das toolchains da lista, e elas não aparecem no sandbox (achado de 2026-11-12) | 1 |
| Harnesses reais no sandbox (2026-11-12): o opencode inicia, mostra a versão e desenha a interface na pasta de trabalho; o hermes precisa de `~/.hermes/installs` (acrescentado ao preset), inicia o próprio gateway dentro do sandbox, mas com as chaves falsas da importação grava a sessão sem modelo e recusa abrir (`storage_unavailable`). Uma sessão completa dos dois depende da chave real do modelo | 1 |
| A proteção do `.git` tem uma brecha: um `.git/commondir` criado pelo agente faz o git do host usar outro diretório git, com outra configuração e outros hooks (confirmado em 2026-11-12). Detalhes em `docs/git.md`. Decidido em 2026-11-13: a correção fica para a etapa 6 | 1 |
| A chave `updates.check: false` no `config.yaml` do hermes desliga a checagem de atualização. **Confirmado em 2026-11-12** (tarefa 2.5): com a chave, a checagem devolve `disabled` sem nenhuma consulta externa; sem ela, tenta consultar o GitHub | 1 |
| O que funciona com user namespace aninhado. **Confirmado em 2026-11-12** (tarefa 11.3): o Chromium headless do Playwright roda dentro do sandbox com o isolamento próprio ligado; montar dentro de um user namespace aninhado falha, então podman sem root e bubblewrap não funcionam | 1 |
| Limites de recurso sem privilégio | 2 |
| Camada de escrita sobre as toolchains com o mesmo overlay dos caches. **Confirmado em 2026-11-12** (tarefa 2.2): `cargo install` e `npm install -g` funcionaram na camada e não alteraram o host | 1 |
| Docker rootless iniciado pelo sandmux, com a visão de arquivos do sandbox e só o socket entregue ao agente | 2 |
| Tráfego do Docker privado passando pelo gateway | 4 |
| GPU funcionando dentro do sandbox. **Confirmado em 2026-11-12**: `nvidia-smi` enxerga a RTX 2080 SUPER da máquina de referência dentro do sandbox | 1 |
| Fidelidade do emulador de terminal | 3 |
| Runtime do opencode aceita a CA local | 5 |
| Custo da interceptação de arquivos | 6 |
| Montagem de pasta num sandbox em execução | 6 |

### Riscos conhecidos e aceitos

- No modo aberta, o conteúdo da pasta de trabalho pode ser enviado para qualquer domínio fora da denylist. A defesa é o registro, o destaque de domínio novo e o bloqueio rápido.
- Com a rede local liberada, serviços locais sem senha ficam ao alcance do agente.
- No MCP de banco, a proteção depende das permissões do usuário cadastrado no banco.
- O HOME sintético compartilhado permite que um projeto afete skills ou memória usadas por outro.
- Com o ssh-agent liberado, o agente pode autenticar nos servidores git cadastrados quantas vezes quiser enquanto a aba vive.
- Cron e canais do hermes ficam parados após reboot até o cofre ser desbloqueado.
- A proteção do `.git` cobre hooks e config, mas outros arquivos da pasta de trabalho também são executados pelo usuário fora do sandbox (`Makefile`, scripts do `package.json`, `.envrc`, tarefas do editor). O que o agente escreve ali precisa ser lido antes de ser executado no host.
- O Docker privado roda fora do Landlock e usa montagens e rede em user namespace, o que amplia a parte do kernel exposta ao agente. É coerente com exploit de kernel fora do escopo.
- Uma ferramenta instalada pelo agente na camada privada das toolchains continua lá nas execuções seguintes do mesmo harness. O host não é afetado.
- Com a GPU liberada por padrão, os drivers de vídeo ficam expostos a todo agente. É coerente com exploit de kernel fora do escopo.
- Num worktree, o agente alcança o histórico e as branches dos outros worktrees do mesmo repositório.
- Uma credencial de cloud montada de propósito é real e fica ao alcance do agente até a credencial temporária existir.
- Pacotes do sistema (`apt install`) não podem ser instalados dentro do sandbox, que não tem root. O caminho é um contêiner no Docker privado.
- Até a etapa 6, arquivos de segredo que já estão na pasta de trabalho (`.env`, chaves) ficam legíveis pelo agente, e até a etapa 4 a rede está aberta e sem registro. Aceito pelo usuário em 2026-11-12: quem usa o sandmux nessas etapas precisa tirar os segredos da pasta.
- Até a etapa 6, um `.git/commondir` criado pelo agente faz o git do host usar outro diretório git, com outra configuração e outros hooks. A montagem não impede criar o arquivo, e cobri-lo quebra o git. Decidido pelo usuário em 2026-11-13: a correção vem com a interceptação de arquivos da etapa 6, e conferir o arquivo ao fim de cada execução foi descartado. Até lá, `docs/git.md` orienta o usuário a conferir o arquivo antes de usar o git no host.
- O que é colocado na pasta de entrada fica visível para todos os agentes até ser apagado.
- Com a escrita no clipboard bloqueada por padrão, o recurso de copiar do próprio harness não funciona até ser liberado.

### Decisões ainda não tomadas

| Decisão | Change |
|---|---|
| Nenhuma no momento | |

As demais decisões levantadas até 2026-11-13 estão resolvidas e registradas nas seções 2 e 4.

Novas decisões em aberto que surgirem ao propor um change devem ser anotadas aqui até serem tomadas.

## 7. Rastreio dos pedidos originais

| Pedido | Onde é atendido |
|---|---|
| Rust com ratatui | Todas as etapas; TUI na etapa 3 |
| Binário único | Substituído por quatro binários (seção 2.1) |
| Interface fácil para subir harnesses | Changes 1, 2 e 5 |
| Sandbox com seccomp e Landlock | Change 1 |
| Pastas com allowlist e denylist, e interface de cadastro | Change 1 (opções de linha de comando) e change 16 (interface) |
| Controle de acesso a arquivos | Change 16 |
| Controle de saída de rede por domínio | Changes 8 e 9 |
| Bloquear a internet, menos a API de inferência | Changes 9 e 13 (modo só inferência) |
| Controlar quais APIs de inferência estão liberadas | Change 13 |
| Controlar quais MCPs estão liberados | Change 17 |
| Interface de rede criada na instalação | Substituída por interface por sandbox, sem privilégio (change 8) |
| Proxy para monitorar e bloquear | Changes 8 e 9 (gateway no daemon) |
| Registro de toda interação com pastas, arquivos e rede | Changes 8 (rede) e 15 (arquivos) |
| Ver acessos novos numa lista e adicionar à denylist | Changes 9 (rede) e 16 (arquivos) |
| Conexões de banco por MCP com credencial injetada | Change 18 |
| Agente confinado numa pasta | Change 1 |
| Nenhum segredo na pasta; injeção no tráfego | Changes 11, 12 e 13 |
| Chave falsa no config do harness, trocada na saída | Changes 1 (importação) e 13 |
| Abas para terminais diferentes | Change 5 |
| Atalhos configuráveis para navegar entre abas | Change 5 |
| Modal com a lista de abas | Change 5 |
| Clique do mouse | Change 5 |
| Nome de aba customizável | Change 5 |
| Temas de cores | Change 6 |
| Modal de chat e aba de conversa | Change 21 |
| Modal que gera e injeta comando shell | Change 22; o terminal de shell vem do preset `shell` (change 1) |
| Configurações persistidas em arquivo | Change 3 |
| Agentes sobrevivem ao fechar | Change 2 |
| API keys e OAuth | Change 13 (API key) e pós-v1 (OAuth) |
| Segredos fora do sandbox; keyring depois | Change 11 e pós-v1 |
| Toda aba com sandbox | Changes 1, 2 e 5 |
| Validação de prompt injection | Pós-v1 |
| Seguro por padrão, mínimo atrito | Baseline embutida (change 1) e camadas de política (change 3) |
| eBPF como otimização, com recuo | Pós-v1; detecção de nível no change 23 |
| `sudo` na instalação | Change 23 |
| Ferramentas comuns de desenvolvedor, como o Docker (pedido de 2026-11-12) | Change 24 (Docker privado), change 1 (instalações globais) e change 25 (`gh` e registries privados) |
