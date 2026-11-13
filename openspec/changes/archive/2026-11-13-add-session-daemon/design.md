# Design

## Context

Veja proposal.md, seção Why. O estado atual que importa para este change:

- O `sandmux run` faz tudo no processo do cliente:
  - lê o ambiente (`Host::detect`: HOME, UID, shell, diretório de dados, pasta atual e variáveis);
  - pergunta sobre a importação na primeira execução;
  - pede ao `sandmux-run --check` o suporte da máquina;
  - pega a trava da camada de escrita do harness;
  - compõe a especificação (`sandmux_policy::compose`), grava-a num memfd e executa o `sandmux-run --spec-fd`. O descritor da trava fica aberto e é herdado pelo lançador.
- O `sandmux-run` é o supervisor: repassa TERM e HUP ao processo 1 do sandbox e termina com o código do comando (125 em falha de montagem, 127 em falha de exec). O processo 1 já usa `PR_SET_PDEATHSIG` com SIGKILL ligado ao supervisor. Quando o supervisor morre, o pid namespace inteiro morre junto.
- Dentro do sandbox, `/run` e `/tmp` são tmpfs novos, e o Landlock nega sockets abstratos do host. Um socket em `$XDG_RUNTIME_DIR` já fica fora do alcance do agente.
- O seccomp do sandbox já nega TIOCSTI e TIOCLINUX.
- Os 126 testes rodam `sandmux run` sem terminal, comparando a saída exata e o código de saída. Esse caminho precisa continuar idêntico (decisão do usuário: sem terminal, o `run` roda direto).

## Goals / Non-Goals

**Goals:**

- Um único lugar compõe e lança sandboxes, usado tanto pelo `run` direto quanto pelo daemon. Nenhuma regra de política é duplicada.
- Reanexar a uma sessão do opencode ou do hermes mostra a interface utilizável sem depender de o harness redesenhar.
- O daemon nunca grava a saída das sessões nem valores de `--pass-env` em disco, nem em log.
- Os testes não tocam no systemd nem no daemon do usuário.

**Non-Goals:**

- Renderizar a tela no daemon para clientes de tamanhos diferentes (o que o tmux faz). Aqui a saída passa crua. A TUI da etapa 3 traz o próprio emulador.
- Responder no lugar do terminal às consultas que o harness faz (atributos, cor de fundo, posição do cursor).
- Configuração em arquivo. O tamanho do histórico é uma opção de linha de comando do `sandmuxd` até o change 3.

## Decisions

### 1. Organização do código

- Crate novo `sandmux-proto`: tipos das mensagens, enquadramento e número da versão do protocolo. Cliente e daemon dependem dele, e ele não depende de política.
- O pacote `sandmux` ganha uma biblioteca (`src/lib.rs`) com o preparo do lançamento: composição, trava da camada, memfd e o comando do `sandmux-run` pronto para executar. O `run` direto e o daemon usam a mesma função.
- O binário `sandmuxd` fica no pacote `sandmux`, ao lado de `sandmux`, `sandmux-run` e `sandmux-probe`, e é encontrado na mesma pasta do cliente, como o `sandmux-run`.

Alternativa descartada: um crate de daemon separado com a lógica de lançamento copiada. Duplicaria a parte mais sensível do projeto.

### 2. Quem compõe a especificação

O daemon compõe, porque a restauração depois do reboot relança sessões sem nenhum cliente presente. O cliente:

- faz as verificações interativas e baratas: preset conhecido, harness instalado e importação na primeira execução, que pergunta no terminal dele;
- envia ao daemon as opções do `run`, a pasta atual, as variáveis da lista segura, o `SHELL` e os valores de `--pass-env`.

O daemon monta um `Host` com o HOME, o UID e o diretório de dados dele, mas com a pasta, as variáveis e o shell do pedido, e chama o mesmo `compose`. Um erro de composição volta ao cliente como mensagem, e o cliente o mostra igual ao `run` direto (pasta perigosa, caminho inexistente). Os avisos voltam do mesmo jeito.

O diretório de dados precisa ser o mesmo nos dois lados, porque a importação roda no cliente e o lançamento no daemon. O cliente envia o dele na saudação, e o daemon recusa um diretório diferente com uma mensagem que mostra os dois. A unidade systemd é gravada com o `XDG_DATA_HOME` do cliente quando ele está definido.

### 3. Protocolo

- Socket Unix de fluxo. Cada quadro tem 4 bytes de tamanho (big-endian), 1 byte de tipo e o conteúdo. O tipo 0 é uma mensagem de controle em JSON. O tipo 1 são bytes de terminal: entrada do cliente ou saída da sessão.
- O primeiro quadro é a saudação: o cliente manda o número do protocolo, a versão do sandmux e o diretório de dados. O daemon responde com o número dele, a versão, o PID e o modo de início. Números diferentes encerram a conexão, e o cliente mostra as duas versões e indica `sandmux daemon restart`.
- Pedidos: criar, listar, anexar, desanexar, encerrar, reiniciar, redimensionar, estado e parar. Eventos do daemon: saída, aviso, erro, sessão encerrada (com o código) e desanexado (com o motivo).
- O número do protocolo é um inteiro, incrementado a cada mudança incompatível. A regra é igualdade exata.

Alternativas descartadas:
- Formato binário (bincode, protobuf): o JSON no controle é fácil de inspecionar, e o volume vai todo no tipo 1, que é cru.
- Aceitar a versão anterior: o usuário escolheu recusar e indicar o reinício.

### 4. Socket e acesso

- O socket fica em `$XDG_RUNTIME_DIR/sandmux/sandmuxd.sock`, numa pasta com modo 0700. Sem `XDG_RUNTIME_DIR`, a pasta é `/tmp/sandmux-<uid>/`, criada com 0700. Uma pasta já existente precisa pertencer ao usuário, ter modo 0700 e não ser link simbólico; caso contrário, o daemon recusa.
- Em cada conexão aceita, o daemon confere o UID do outro lado com `SO_PEERCRED` e fecha a conexão se for outro UID.
- A instância única é garantida por um `flock` num arquivo de trava na mesma pasta, tomado antes de criar o socket. Um segundo daemon falha no `flock` e termina com erro. Um socket órfão, deixado por um daemon morto, é removido por quem conseguir a trava.
- Os testes usam um `XDG_RUNTIME_DIR` temporário. Cada teste tem o próprio daemon.

### 5. Início do daemon

O cliente tenta conectar. Sem resposta, ele decide como subir o daemon:

1. Com `SANDMUX_DAEMON_START=direct`, ou sem systemd de usuário (`systemctl --user show-environment` falha), o cliente sobe o `sandmuxd` como processo próprio. O processo é criado com `setsid`, com entrada e saída em `/dev/null` e com o log na pasta de estado. O cliente espera o socket responder, por até 5 segundos.
2. Com systemd de usuário, o cliente:
   - grava `~/.config/systemd/user/sandmuxd.service`, com `ExecStart` apontando para o `sandmuxd` instalado, `Restart=on-failure` e `WantedBy=default.target`, mais `Environment=XDG_DATA_HOME=…` quando o cliente o tem;
   - roda `systemctl --user daemon-reload` e `enable --now`;
   - avisa na primeira vez.

   Se a unidade existe mas o `ExecStart` aponta para outro binário, o cliente a regrava e avisa.

Os testes de integração sempre definem `SANDMUX_DAEMON_START=direct`, e nenhum teste chama o `systemctl` do usuário. A instalação da unidade é testada pelo conteúdo do arquivo gerado, num HOME falso, sem habilitá-la.

O log do daemon (eventos, sem saída de sessão nem valores de variáveis) vai para o stderr, que no systemd é o journal. No início direto, ele vai para `$XDG_STATE_HOME/sandmux/sandmuxd.log`.

### 6. Execução das sessões

- O daemon usa threads do sistema e I/O bloqueante, sem runtime assíncrono: uma thread de leitura por sessão (o PTY) e duas por cliente (leitura do socket e escrita, com fila limitada). O estado fica sob um único mutex. Com poucas sessões e poucos clientes, isso tem menos peças móveis que um runtime assíncrono (decidido na implementação, 2026-11-13). Um cliente cuja fila enche é desanexado, para nunca travar a sessão.
- O lançamento de sandboxes acontece numa thread própria, que vive enquanto o daemon vive. O motivo: o `PR_SET_PDEATHSIG` dispara quando termina a *thread* que criou o filho, e não o processo.
- **Descritores**: o `openpty` não marca o PTY com `CLOEXEC`, e o lançador não fecha descritores extras antes do exec. No processo filho, antes do exec do `sandmux-run`, o daemon marca todos os descritores acima de 2 com `CLOEXEC` (`close_range`) e libera só o memfd da especificação e a trava da camada. Sem isso, o PTY de uma sessão e o memfd com valores de `--pass-env` vazariam para o sandbox de outra. O mesmo filho restaura a máscara de sinais e as disposições padrão, que o daemon alterou.
- Para cada sessão, o daemon:
  - abre um PTY;
  - executa o `sandmux-run --spec-fd` com o lado escravo como entrada, saída e erro. No processo filho, antes do exec, chama `setsid`, `TIOCSCTTY` e `PR_SET_PDEATHSIG` com SIGKILL;
  - herda a trava da camada e o memfd como hoje.

  A cadeia "daemon morre → `sandmux-run` recebe SIGKILL → processo 1 recebe SIGKILL" garante que nenhum sandbox sobrevive ao daemon.
- O fim da sessão vem do `wait` do `sandmux-run`: o código do comando, ou 125 e 127 nas falhas de lançamento. A mensagem do lançador já sai no PTY e fica no histórico.
- Encerrar: o daemon envia SIGTERM e SIGHUP ao `sandmux-run`, que os repassa ao comando. O SIGHUP imita o terminal que fecha, porque shells interativos ignoram SIGTERM (decidido na implementação). Se o harness não terminar em 5 segundos, o daemon envia SIGKILL ao `sandmux-run`, e o PDEATHSIG derruba o resto.
- Parada e nova subida: o cliente de `daemon stop` e `daemon restart` espera o *processo* do daemon terminar, não só o socket sumir, porque o daemon ainda escoa as filas dos clientes por um instante depois de apagar o socket. Um daemon que não consegue a trava de instância única desiste na hora (achado na implementação: com espera pela trava, um daemon iniciado em duplicidade assumia o lugar depois de um `stop`).

### 7. Saída: repasse cru, emulador para reanexar

- A saída do PTY passa pelo filtro de OSC 52 (decisão 8). Depois vai crua aos clientes anexados e alimenta um `vt100::Parser` da sessão, com histórico igual ao limite configurado.
- Ao anexar, o daemon envia:
  1. as linhas do histórico, formatadas (texto com SGR), terminadas por quebra de linha, para entrarem na rolagem do terminal do cliente;
  2. o estado da tela do parser (`state_formatted`): tela principal ou alternativa, conteúdo, cursor e modos que o `vt100` acompanha;
  3. os modos que o `vt100` não acompanha, registrados por um rastreador próprio que lê o mesmo fluxo:
     - a pilha do protocolo de teclado do kitty (`CSI > n u`, `CSI < u`, `CSI = n u`);
     - eventos de foco (`?1004`);
     - colagem entre marcadores (`?2004`);
     - os modos de mouse (`?1000`, `?1002`, `?1003`, `?1006`).

  Enquanto o reenvio não termina, a saída nova da sessão espera numa fila do cliente, para não intercalar.
- Ao desanexar, o cliente desfaz esses modos no terminal dele: sai da tela alternativa, mostra o cursor, desliga mouse, foco e colagem, e retira o protocolo do kitty. Depois devolve o modo de linha.

Alternativa descartada: renderizar tudo a partir do emulador, como o tmux. Resolve clientes de tamanhos diferentes, mas perde as sequências que o emulador não conhece e piora a fidelidade com as TUIs dos harnesses. A etapa 3 traz o próprio emulador na TUI.

### 8. Filtro de sequências de escape

- O filtro é uma máquina de estados sobre o fluxo de saída. Reconhece OSC iniciado por `ESC ]` ou pelo C1 codificado em UTF-8 (`C2 9D`) e terminado por BEL, `ESC \` ou `C2 9C`, mesmo quando a sequência chega dividida entre leituras. O byte cru `0x9d` não é tratado como C1: num fluxo UTF-8 ele é byte de continuação de caracteres comuns (decidido na implementação).
- Sequências OSC 52: a consulta (`52;<sel>;?`) é sempre retirada. A gravação é retirada, a menos que a sessão tenha `--allow-clipboard`.
- Um OSC que passa de 1 MiB sem terminador é descartado inteiro, e o filtro volta ao estado normal no terminador seguinte.
- As outras sequências passam sem mudança.

### 9. Entrada e tecla de prefixo

- O cliente põe o terminal em modo cru e lê a entrada em blocos. Reconhece Ctrl+b de duas formas:
  - o byte 0x02;
  - a forma do protocolo do kitty (`CSI 98 ; 5 u`, com ou sem o campo de tipo de evento), porque o opencode liga esse protocolo e o Ctrl+b deixa de chegar como 0x02.
- Depois do prefixo, `d` (em qualquer das duas formas) desanexa e um segundo Ctrl+b envia um Ctrl+b na mesma forma em que chegou. Qualquer outra tecla é descartada.
- O resto da entrada vai ao daemon como bytes. As respostas do terminal do cliente às consultas do harness também vão: com vários clientes anexados, o harness pode receber mais de uma resposta (ver Riscos).
- O SIGWINCH do cliente vira um pedido de redimensionamento. O daemon aplica ao PTY e ao parser o menor número de colunas e o menor de linhas entre os clientes anexados.

### 10. Modo do `run`

- `sandmux run` cria a sessão quando a entrada e a saída padrão são terminais. Caso contrário, roda direto, pelo caminho atual, sem nenhuma mudança visível.
- No modo sessão:
  - o cliente mostra os avisos e anexa;
  - termina com o código do harness quando a sessão termina enquanto está anexado;
  - termina com 0 e uma linha de como reanexar quando é desanexado por Ctrl+b d, `sandmux detach` ou SIGTERM;
  - termina sem mensagem quando é desanexado por SIGHUP (terminal fechado).

### 11. Lista de sessões e restauração

- O arquivo é `<diretório de dados>/sessions.json`, com modo 0600, gravado de forma atômica (arquivo temporário e `rename`) a cada mudança de estado. Para cada sessão, guarda:
  - o identificador, o nome, o harness e a pasta;
  - as opções do `run` (`--ro`, `--rw`, `--repo`, `--kvm`, `--no-gpu`, `--allow-clipboard` e os argumentos depois de `--`);
  - os nomes de `--pass-env`;
  - o estado: viva, encerrada (com o código) ou parada (com o motivo).

  Guarda também o próximo identificador.
- A parada do daemon (`stop`, `restart`, SIGTERM do systemd no logout ou no desligamento) encerra os harnesses e mantém as sessões vivas marcadas como vivas. Um SIGKILL deixa o arquivo como estava na última mudança, que também diz viva.
- Ao subir, o daemon lê o arquivo e trata cada sessão viva assim:
  - com nomes de `--pass-env`, ela fica parada com o motivo "precisa de --pass-env: NOMES";
  - se a pasta não existe, fica parada com o motivo;
  - nos demais casos, é relançada sem cliente.

  As encerradas voltam encerradas.
- `sandmux restart` envia de novo o ambiente e os valores de `--pass-env` do terminal que pediu.

### 12. Testes

- Os testes de integração sobem um daemon por teste, com `XDG_RUNTIME_DIR`, `XDG_DATA_HOME` e HOME temporários e `SANDMUX_DAEMON_START=direct`.
- Um auxiliar de teste abre um PTY, roda o cliente nele, escreve teclas e lê a tela por um `vt100::Parser`, para os cenários de anexar, prefixo, redesenho e tamanho.
- Os cenários de protocolo (versão, UID, pedidos) usam um cliente de teste que fala o protocolo direto.
- Para o cenário de versões diferentes, o `sandmuxd` aceita a variável `SANDMUX_PROTOCOL_OVERRIDE`, que só muda o número anunciado. Ela não afrouxa nenhuma verificação.

## Risks / Trade-offs

- **Respostas duplicadas a consultas do terminal.** Com dois clientes anexados, uma consulta do harness (atributos, cor de fundo, posição do cursor) é respondida pelos dois terminais. → Mitigação: os harnesses suportados fazem as consultas na partida, e a sessão costuma começar com um só cliente. A TUI da etapa 3 vai responder no lugar do terminal. Vai para as verificações da etapa.
- **Sessão restaurada sem cliente.** Na restauração, o harness sobe sem terminal anexado, e consultas feitas na partida ficam sem resposta. → Mitigação: verificar se o opencode e o hermes sobem e funcionam ao anexar depois. Se algum travar, o daemon passa a responder às consultas básicas de atributos com um valor fixo, sem mudar a spec.
- **Clientes maiores que a sessão.** Um terminal maior que o menor anexado mostra a sessão no canto superior esquerdo, com sobras na tela. → Aceito nesta etapa, coerente com o repasse cru.
- **Reenvio de 10.000 linhas ao anexar.** Pode levar uma fração de segundo num terminal lento. → Aceito. O limite é ajustável.
- **A unidade systemd muda o sistema do usuário.** → O cliente avisa na primeira vez, `sandmux daemon disable` remove tudo, e os testes nunca tocam no systemd do usuário.
- **Diretório de dados diferente entre cliente e daemon** (por exemplo, `XDG_DATA_HOME` definido só no shell). → A saudação compara os dois e recusa com as duas pastas na mensagem. A unidade é gravada com o `XDG_DATA_HOME` do cliente.
- **Trava da camada com sessões longas.** Com o daemon, duas sessões do mesmo harness ficam abertas por mais tempo, e a segunda roda com camada temporária. → Comportamento já especificado e avisado. Fica registrado como ponto a observar.
- **Daemon sem rede e sem cofre nesta etapa.** Ele ainda não filtra nada do tráfego. → Fora do escopo, nas etapas 4 e 5.

## Migration Plan

- A primeira chamada de `sandmux run` num terminal depois da atualização instala a unidade systemd, sobe o daemon e avisa. Scripts sem terminal não mudam.
- Para voltar ao comportamento anterior, rode `sandmux daemon disable` e use o `run` sem terminal. Sessões vivas são encerradas pelo `disable`.
- O README ganha os comandos novos, e o `sandmux doctor` passa a mostrar o estado do daemon e da unidade.
