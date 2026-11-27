# Segurança

Este documento é para quem precisa decidir se pode confiar no sandmux: o modelo de ameaça, o que cada camada impede, o que fica fora do escopo e os limites conhecidos. Como as camadas funcionam está em [architecture.md](architecture.md).

O sandmux está em desenvolvimento, antes da v1. Tudo o que está aqui descreve o comportamento das specs vigentes (`openspec/specs/`), cobertas por testes.

## Modelo de ameaça

**A ameaça:** um agente de código manipulado por prompt injection (num arquivo do projeto, numa página, na resposta de uma ferramenta) que tenta ativamente:

- ler segredos: chaves SSH, credenciais de cloud, tokens de outros programas, `.env`, chaves de API;
- mandar dados para fora: o código, os segredos que conseguir, o conteúdo do disco;
- deixar algo plantado que rode fora do sandbox: um hook do git, um script de login do shell;
- alcançar o resto da máquina: outros processos, sockets do host, o Docker do host.

**O que se supõe confiável:**

- o kernel Linux e o hardware;
- você, o usuário: o que você libera de propósito vale;
- o host fora do sandbox: os binários do sandmux, o daemon e os programas que rodam como o seu usuário fora de uma aba;
- os destinos que você liberou ou para os quais existe um segredo no cofre.

O agente roda com o seu UID. A barreira não é outro usuário: são os namespaces, o Landlock, o seccomp e o gateway.

## O que cada camada impede

| O agente tenta | O que impede | Detalhes |
|---|---|---|
| Ler `~/.ssh`, `~/.aws`, `~/.config/gh` e o resto do HOME | O HOME é sintético; do HOME real só entram a pasta de trabalho, as toolchains conhecidas (com os arquivos de credencial vazios) e o que você acrescentar | [files.md](files.md) |
| Ler `.env`, `*.pem` ou `id_rsa` da pasta de trabalho | Arquivos com cara de segredo não abrem (`ENOENT`), a menos que o próprio agente os tenha criado | [files.md](files.md#arquivos-com-cara-de-segredo) |
| Ler `/etc/shadow`, chaves de host, configurações de VPN | Esses arquivos aparecem vazios; `passwd` e `group` são gerados | [architecture.md](architecture.md#arquivos) |
| Roubar a chave de API do provedor | A chave real nunca entra: o agente tem um falso, trocado pelo real no gateway só quando o destino é o dono da chave | [vault.md](vault.md#troca-da-chave-no-tráfego) |
| Mandar dados para um destino de exfiltração | A denylist vem com um catálogo de destinos conhecidos, ligado; os modos `inferencia`, `custom` e `sem-rede` restringem mais; toda conexão é registrada, e um domínio novo é destacado | [network.md](network.md#modos) |
| Alcançar os metadados de cloud (`169.254.169.254`) | Bloqueados em todo modo, junto com o QUIC e o ICMP | [network.md](network.md#bloqueios-fixos) |
| Plantar um hook ou um remote no `.git` | `.git/hooks` e `.git/config` ficam em somente leitura, e criar `.git/commondir` é negado | [git.md](git.md#o-que-fica-protegido) |
| Plantar algo no `.bashrc` ou em `~/.local/bin` do host | Esses caminhos não podem ser montados com escrita; a pasta de configuração do sandmux também não | [config.md](config.md#caminhos-que-o-host-executa) |
| Falar com o ssh-agent, o D-Bus, o Docker, o Wayland ou o X11 do host | Os sockets do host não existem no sandbox, e o Landlock nega os sockets abstratos de fora | [architecture.md](architecture.md#as-camadas-do-sandbox) |
| Ver, sinalizar ou depurar processos do host | O pid namespace só mostra o sandbox, e o Landlock nega sinais para fora | [architecture.md](architecture.md#as-camadas-do-sandbox) |
| Escrever no terminal do usuário (injetar comandos) | Inserir caracteres na fila de entrada do terminal é negado | [architecture.md](architecture.md#as-camadas-do-sandbox) |
| Ganhar privilégio | Sem capabilities, `setuid` não eleva, e montagem, `bpf`, módulos, `kexec`, `io_uring` e afins são negados pelo seccomp | [architecture.md](architecture.md#as-camadas-do-sandbox) |
| Esgotar a memória ou os processos da máquina | Teto de memória e de processos por lançamento; o kernel mata só processos daquele sandbox | [limits.md](limits.md) |
| Usar o Docker do host | O socket do host nunca entra; cada pasta tem um Docker sem root próprio, com a mesma visão de arquivos e a mesma rede do sandbox | [docker.md](docker.md) |
| Ler a senha do banco | Quem conecta é o daemon, fora do sandbox; a senha nunca entra | [database.md](database.md) |

## Fora do escopo

- **Exploit de kernel.** O sandmux usa o kernel como barreira. Um agente que explore uma falha do kernel sai do sandbox. A GPU, liberada por padrão, e o Docker privado, que usa montagens e rede em user namespace, ampliam a parte do kernel exposta ao agente.
- **O que você libera de propósito:**
  - `--rw` e `--ro` de uma pasta com credenciais (o `run` avisa a cada execução);
  - `--pass-env`, que põe o valor real de uma variável no sandbox quando ela não tem segredo no cofre;
  - `--decrypt`, que amplia a decifração do TLS;
  - o modo `aberta`, o padrão, em que tudo fora da denylist passa.
- **O host fora do sandbox.** Um processo do seu usuário com privilégio para ler a memória de outros processos alcança o cofre aberto. O `sandmux update` roda o atualizador oficial do harness no host, fora do sandbox ([sessions.md](sessions.md#atualizar-o-harness)).
- **O que acontece no destino.** O sandmux controla para onde o tráfego vai e troca as chaves; ele não decide o que o provedor, a cloud ou o banco fazem com um pedido autorizado.
- **Arquivos que você executa depois.** Além de `.git/hooks` e `.git/config`, a pasta de trabalho tem arquivos que você roda no host: `Makefile`, scripts do `package.json`, `.envrc`, tarefas do editor. O que o agente escreve ali precisa ser lido antes.

## Limites conhecidos

**Rede:**

- No modo `aberta`, o conteúdo da pasta de trabalho pode ser enviado para qualquer domínio fora da denylist. A defesa é o registro, o destaque de domínio novo e o bloqueio rápido. Quem precisa de garantia usa os modos `custom` ou `inferencia`.
- No modo `aberta`, um destino da denylist atrás da mesma CDN de um site liberado só é pego pelo SNI ou pelo `Host`. Um cliente que manda SNI vazio para um IP de CDN passa.
- No modo `aberta`, uma conexão direta por IP, sem consulta de DNS, passa, registrada como "sem domínio". Nos modos restritos, ela é bloqueada.
- Com a rede local liberada no modo `aberta`, serviços locais sem senha ficam ao alcance do agente.
- Uma regra de rede nova vale nas conexões seguintes; as já abertas não são cortadas.

**Segredos:**

- O falso é útil contra o dono do segredo: o agente pode usar a chave de inferência pelo gateway, e gastar créditos, por desenho. Cada uso fica no registro, com o nome do segredo.
- A troca é só nos cabeçalhos. Provedores que pedem a chave no corpo, ou programas que fixam o certificado do servidor, não são atendidos.
- As identidades de cloud valem em toda aba, e o limite é só o IAM (ou o RBAC, no Kubernetes): uma identidade que pode criar chaves ou ler um cofre de segredos da cloud deixa o agente obter uma credencial real pela própria API ([cloud.md](cloud.md#o-que-o-agente-pode-fazer)).
- No login do Antigravity CLI, o token tem o escopo `cloud-platform`: o agente da aba do `agy` pode agir no Google Cloud do usuário pelo gateway, com cada uso no registro ([agy.md](agy.md#login-do-google)).
- Com chaves SSH escolhidas, o agente assina quantas vezes quiser enquanto a aba vive; cada assinatura fica no registro, e o SSH de saída só chega aos servidores dessas chaves.
- No MCP de banco, a proteção depende das permissões do usuário cadastrado no banco.

**Arquivos e estado:**

- O agente pode ler, alterar e apagar o que está na pasta de trabalho. É a pasta real.
- O HOME sintético é compartilhado entre as abas do mesmo harness: um projeto pode afetar skills ou a memória usadas por outro.
- Num worktree do git, o agente alcança o histórico e as branches dos outros worktrees do mesmo repositório.
- Com o caminho do kernel, um segredo criado no host durante a sessão fica legível por até cerca de 1 s, até ser marcado.
- O que é colocado na pasta de entrada (`~/inbox`) fica visível para todos os agentes até ser apagado.

**Docker privado:**

- Um contêiner roda no user namespace do `rootlesskit`, sem o Landlock e sem o supervisor de aberturas. A visão de arquivos é a mesma do sandbox, e com o auxiliar root o BPF LSM confina os contêineres ([docker.md](docker.md#confinamento-por-ebpf)).
- Sem o confinamento por eBPF, as proteções feitas por montagem (segredos cobertos, `.git` em somente leitura) caem para um contêiner com `--privileged`, e um arquivo criado com o Docker já rodando não é coberto. Com o confinamento, o kernel nega pelo arquivo, com uma janela de cerca de 200 ms para os arquivos novos ([docker.md](docker.md#limites)).

## Como verificar

- **`sandmux doctor`:** o que a máquina oferece (user namespaces, Landlock, BPF LSM, auxiliar root, tetos, Docker) e o que falta.
- **`sandmux enter <id>`:** um shell dentro do sandbox de uma aba viva, com o que o agente vê ([sessions.md](sessions.md#shell-dentro-do-sandbox-de-uma-aba)).
- **`sandmux net <id>` e `sandmux files <id>`:** o registro de rede e de arquivos da aba, com os bloqueios ([network.md](network.md#registro), [files.md](files.md#registro)).
- **`sandmux net rules <id>` e `sandmux config show`:** a política efetiva e a origem de cada regra.
- **As specs e os testes:** cada comportamento deste documento é um requisito em `openspec/specs/`, e a suíte de testes inclui tentativas de fuga do sandbox.
