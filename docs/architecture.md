# Arquitetura

Este documento explica como o sandmux confina um agente, para quem ainda não conhece o projeto. Cada seção termina com o link para o documento que tem os detalhes. O que cada camada protege, e o que fica de fora, está em [security.md](security.md).

## Visão geral

O sandmux é um multiplexador de terminal no estilo do tmux. Cada aba roda um harness de agente (opencode, Claude Code, hermes, oh-my-pi, Antigravity CLI) ou um shell, e toda aba tem sandbox: não existe aba sem confinamento.

Dentro da aba, o agente enxerga:

```text
host                                  sandbox
~/projetos/app/   <-- mesma pasta --> ~/projetos/app/    leitura e escrita
~/ (resto)                            ~/ (HOME sintético, separado)
/usr, /etc, ...   ------------------> somente leitura, /etc com os segredos cobertos
rede              ---- gateway ----->  tudo passa pelo gateway, registrado
chaves reais      ----   cofre   ---->  valores falsos
```

A pasta de trabalho é a pasta real do host, no mesmo caminho: o que o agente escreve aparece no seu editor na hora. O resto do HOME não existe lá dentro. A rede sai por um gateway que registra e filtra. Nenhuma chave real entra: o agente recebe um valor falso, e o gateway troca pelo real no tráfego.

Tudo isso usa primitivas do kernel Linux (namespaces, Landlock, seccomp), sem VM, sem contêiner e sem root para o uso normal.

## Os binários e as fronteiras de confiança

```text
+--------------+  socket   +----------------------+  socket  +----------------+
| sandmux      |<--------->| sandmuxd             |<-------->| sandmux-helper |
| cliente      |  do       | daemon de sessões:   |          | auxiliar root, |
| (CLI e TUI)  |  usuário  | PTYs, cofre,         |          | opcional:      |
+--------------+           | política, registro   |          | BPF LSM        |
                           +----------+-----------+          +----------------+
                                      | especificação de lançamento
                                      v
                           +----------------------+
                           | sandmux-run          |  fica no host:
                           | lançador e           |  gateway de rede e
                           | supervisor           |  supervisor de arquivos
                           +----------+-----------+
                                      | namespaces, Landlock, seccomp
                                      v
                           +----------------------+
                           | harness (agente)     |  sandmux-chat e
                           | dentro do sandbox    |  sandmux-db-mcp também
                           +----------------------+  rodam aqui
```

| Binário | Papel | Onde roda |
|---|---|---|
| `sandmux` | Linha de comando e TUI. Não toca em segredo nem em sandbox | No host, como o usuário |
| `sandmuxd` | Daemon de sessões, um por usuário: mantém os terminais das abas, o cofre aberto na memória, a política efetiva e os registros | No host, como o usuário |
| `sandmux-run` | Lançador: aplica uma especificação de lançamento pronta. Depois fica como supervisor do sandbox, com o gateway de rede e o supervisor de aberturas de arquivo | No host, como o usuário |
| `sandmux-helper` | Auxiliar root opcional: prende programas BPF LSM, só para restringir, a cgroups e user namespaces de quem pede | No host, como root |
| `sandmux-chat` | A aba de chat com os modelos | Dentro do sandbox |
| `sandmux-db-mcp` | Repassador do MCP de banco: copia o stdio para um socket do daemon | Dentro do sandbox |

As fronteiras:

- **O cliente** fala com o daemon por um socket numa pasta só do usuário (modo 700). O daemon confere o UID de cada conexão (`SO_PEERCRED`) e recusa outro usuário.
- **O daemon** não tem código de interface. Ele resolve a política e manda ao lançador uma especificação completa.
- **O lançador** não conhece harness nem política. Ele monta exatamente o que a especificação descreve e recusa uma especificação malformada, de versão desconhecida ou com campo que não entende.
- **O auxiliar root** aceita seis pedidos, identifica quem pede pelas credenciais do socket, só nega operações e desfaz tudo quando a conexão fecha. Ele não tem rede, TLS nem banco.
- **O sandbox** não alcança o socket do daemon nem o do auxiliar: `/run` e `/tmp` são novos lá dentro, e o Landlock nega os sockets abstratos do host.

Detalhes: [sessions.md](sessions.md#socket-e-acesso), [launch-spec.md](launch-spec.md), [helper.md](helper.md).

## Do comando à aba

```text
sandmux run opencode
  |
  | 1. cliente: verificações, importação da primeira execução, pedido ao daemon
  v
sandmuxd
  | 2. resolve as camadas da política (baseline, preset, global, perfil,
  |    projeto, opções do run) e compõe a especificação de lançamento
  v
sandmux-run --spec-fd N
  | 3. confere o suporte do kernel; sem ele, recusa (nunca sobe sem sandbox)
  | 4. cria user, mount, pid e network namespaces
  | 5. monta a visão: pasta de trabalho, HOME sintético, sistema em somente
  |    leitura, /etc gerado e coberto, toolchains com camada de escrita
  | 6. aplica o Landlock e o filtro seccomp, sem capabilities, sem setuid
  | 7. fica fora do sandbox como supervisor: gateway e aberturas de arquivo
  v
harness, com o terminal da sessão do daemon
```

Num terminal, a aba é uma sessão do daemon: ela continua viva quando você fecha o terminal, como no tmux, e você volta com `sandmux attach`. Sem terminal (script, pipe, CI), o `run` roda direto, em primeiro plano, sem daemon, com o mesmo sandbox.

Como o sandbox é a barreira, os harnesses rodam sem as perguntas de permissão deles; `harness-asks = true` as devolve ([config.md](config.md#perguntas-do-harness)).

Detalhes: [sessions.md](sessions.md), [launch-spec.md](launch-spec.md).

## As camadas do sandbox

```text
  agente (opencode, Claude Code, hermes, ...)
     |
     |  chamadas de sistema                 tráfego de rede
     v                                         v
+-------------------------------+   +------------------------------+
| seccomp: nega montagem, bpf,  |   | network namespace: só lo e   |
| perf, módulos, io_uring, ...; |   | um TUN; nenhuma rota direta  |
| manda as aberturas de arquivo |   | para fora                    |
| ao supervisor                 |   +--------------+---------------+
+-------------------------------+                  |
| Landlock: o que pode ser      |                  v
| lido, escrito e executado;    |   +------------------------------+
| escopo de sockets abstratos,  |   | gateway (no supervisor):     |
| sinais e ioctl                |   | DNS, política de domínios,   |
+-------------------------------+   | decifração só onde há        |
| mount namespace: o que existe |   | segredo, troca de chave,     |
| (HOME sintético, /etc         |   | registro                     |
| coberto, /dev mínimo)         |   +--------------+---------------+
+-------------------------------+                  |
| user e pid namespaces: mesmo  |                  v
| UID, sem capabilities, só os  |          internet e rede local
| processos do sandbox          |
+-------------------------------+
| cgroup: teto de memória e de processos           |
+--------------------------------------------------+
```

- **user e pid namespaces:** o agente roda com o seu UID, sem capabilities, e só vê os processos do próprio sandbox. Programas setuid não elevam privilégio. Ele pode criar user namespaces aninhados (navegadores headless que se isolam funcionam), mas continua sem montar nada.
- **mount namespace:** define o que existe para o agente. Só a pasta de trabalho, o HOME sintético, o sistema em somente leitura, as toolchains e o que você acrescentou com `--ro` ou `--rw`.
- **Landlock:** define o que pode ser feito com o que existe. Também impede a conexão a sockets abstratos do host, sinais a processos de fora e `ioctl` em dispositivos fora da lista.
- **seccomp:** nega montagem e troca de raiz, `bpf`, `perf_event_open`, carga de módulos, `kexec`, o chaveiro do kernel, `userfaultfd` e `io_uring`, com erro de permissão. Também manda cada abertura de arquivo ao supervisor.
- **cgroup:** teto de memória (75% da RAM por padrão) e de 8192 processos e threads por lançamento. Passou do teto, o kernel mata processos só daquele sandbox.

Detalhes: [files.md](files.md), [network.md](network.md), [limits.md](limits.md).

## Arquivos

- **Pasta de trabalho:** a pasta real, no mesmo caminho, com leitura e escrita. Ser o HOME, a raiz ou uma pasta que contém o HOME é recusado. `.git/hooks` e `.git/config` ficam em somente leitura, para que nada plantado pelo agente rode quando você usa o git no host ([git.md](git.md#o-que-fica-protegido)).
- **HOME sintético:** um por harness, guardado no diretório de dados do sandmux, com a configuração importada do harness e os segredos trocados por valores falsos.
- **Toolchains e caches:** uma lista conhecida (nvm, bun, cargo, rustup, uv, pyenv, go, linuxbrew) entra lida do host e escrita numa camada privada do harness. `cargo install` e `npm install -g` funcionam sem alterar o host. Os arquivos de credencial delas aparecem vazios.
- **`/etc`:** `passwd` e `group` gerados; `shadow`, `sudoers`, chaves privadas de host, configurações de VPN e de Wi-Fi, e outros arquivos sensíveis aparecem vazios. `/proc`, `/tmp` e `/run` são novos. O `/dev` tem só o mínimo, mais a GPU.
- **Arquivos com cara de segredo:** na pasta de trabalho, `.env`, `*.pem`, `id_rsa*` e afins não abrem (`ENOENT`), a menos que o próprio agente os tenha criado ([files.md](files.md#arquivos-com-cara-de-segredo)).
- **Interceptação das aberturas:** um filtro seccomp com notificação manda cada `open` ao supervisor. O supervisor abre o arquivo ele mesmo, dentro da raiz do sandbox, confere as regras pelo arquivo que de fato abriu e entrega o descritor ao processo. Trocar o caminho na memória depois da leitura não muda nada. Cada abertura nas pastas do agente vai para o registro, e o `file-deny` vale na abertura seguinte, sem relançar o agente.
- **Pelo kernel:** com o auxiliar root e o BPF LSM, o kernel decide e registra as aberturas da aba no lugar do supervisor, pelo inode dos arquivos marcados ([files.md](files.md#pelo-kernel)).

Detalhes: [files.md](files.md), [config.md](config.md#caminhos-que-o-host-executa).

## Rede

```text
sandbox                               supervisor (sandmux-run, no host)
+---------------------------+         +-------------------------------+
| lo                        |  TUN    | gateway                       |
| sandmux0  10.0.2.100/24  -+-------->|   TCP e UDP refeitos pelo host|
| DNS 10.0.2.3              |         |   DNS pelo resolvedor do host |
+---------------------------+         |   política e registro         |
                                      +-------------------------------+
```

- **Namespace de rede próprio, sem privilégio.** O processo 1 do sandbox cria um TUN e passa o descritor ao supervisor. Não há rota direta para fora: toda conexão termina numa pilha TCP/IP em espaço de usuário e é refeita a partir do host.
- **DNS do gateway.** Cada consulta é registrada, e a resposta liga o IP ao nome nas conexões seguintes. Um nome negado recebe NXDOMAIN.
- **Quatro modos:** `aberta` (o padrão: tudo passa, menos a denylist, que já vem com um catálogo de destinos de exfiltração), `inferencia` (só os provedores de inferência), `custom` (só a allowlist) e `sem-rede`. Em todo modo, metadados de cloud, QUIC e ICMP são bloqueados. Uma regra nova vale na conexão seguinte.
- **Decifração só onde há segredo.** O gateway decifra o TLS apenas dos domínios donos de segredos do cofre e dos que você pedir. Cada lançamento gera a própria CA, que nunca toca o disco. O resto passa cifrado de ponta a ponta.
- **Registro.** Cada conexão, consulta de DNS e pedido decifrado vai para `~/.local/state/sandmux/net/`, com o domínio, o destino, o volume, o processo e o bloqueio. Nos pedidos decifrados, também o método e o caminho; nunca cabeçalhos nem corpo.

Detalhes: [network.md](network.md).

## Segredos

- **Cofre:** um arquivo cifrado com a sua senha (Argon2id e XChaCha20-Poly1305), aberto pelo daemon e guardado só na memória enquanto está desbloqueado. Opcionalmente, a senha fica no keyring do sistema ([vault.md](vault.md)).
- **Valores falsos:** cada segredo tem um falso (`sandmux-fake-` e 32 caracteres). É o falso que entra no ambiente do harness e no HOME sintético.
- **Troca no tráfego:** num pedido decifrado, o gateway troca o falso pelo real nos cabeçalhos, só quando o domínio do pedido é dono do segredo. O falso mandado a outro destino segue falso. O real fica na memória do gateway só durante a conexão.
- **Git por SSH:** um agente SSH próprio da sessão assina com as chaves escolhidas, guardadas no cofre, e o SSH de saída só chega aos servidores delas ([git.md](git.md)).
- **Credenciais de cloud:** o agente recebe uma credencial falsa por identidade de GCP, Kubernetes ou AWS, e o gateway troca o token, ou reassina o pedido, no caminho até a cloud ([cloud.md](cloud.md)).
- **Banco de dados:** as credenciais ficam no cofre, e o MCP de banco do sandmux chega ao banco pelo daemon, que conecta de fora do sandbox; a senha nunca entra ([database.md](database.md)).

Detalhes: [vault.md](vault.md#troca-da-chave-no-tráfego).

## Níveis de proteção

O lançador confere o suporte do kernel antes de cada lançamento e recusa quando falta algo. Não existe opção para lançar sem confinamento.

```text
kernel com Landlock ABI 6 (6.12 ou mais novo)
   +-- sem o auxiliar root --> sandbox completo; aberturas pelo supervisor
   +-- com o auxiliar root --> o mesmo, mais: aberturas da aba decididas e
                               registradas pelo kernel (BPF LSM), execuções
                               e conexões observadas pelo kernel, e o
                               Docker privado confinado por BPF LSM

kernel com Landlock ABI 4 ou 5 (6.7 a 6.11)
   +-- com o auxiliar root e o BPF LSM ativos --> o auxiliar completa o
   |                               escopo de sockets abstratos, de sinais
   |                               e de ioctl que o Landlock não tem
   +-- sem eles ----------------> recusa subir o agente

sem user namespaces sem privilégio ----------> recusa subir o agente
```

O auxiliar root é o único pedaço que roda como root. Ele só restringe, só os cgroups e namespaces de quem pede, e só enquanto a conexão do pedido está aberta. Quando ele reinicia, a aba de uma sessão do daemon fica congelada até as regras voltarem. O `sandmux doctor` mostra o que a máquina oferece e o que falta.

Detalhes: [helper.md](helper.md), [install.md](install.md#ubuntu-2404).

## Daemon e persistência

- **Um daemon por usuário**, iniciado pelo systemd de usuário, ou como processo próprio sem systemd.
- **As abas sobrevivem** ao terminal fechado e ao reinício do daemon: a lista de sessões fica no diretório de dados, e as abas voltam com as mesmas opções. Depois de um reboot, voltam esperando a senha do cofre, quando precisam dele.
- **Docker privado:** cada pasta de trabalho tem um Docker sem root próprio, separado do Docker do host, com a mesma visão de arquivos e a mesma rede do sandbox ([docker.md](docker.md)).

Detalhes: [sessions.md](sessions.md#como-o-daemon-sobe).
