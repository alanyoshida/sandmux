# Tasks

## 1. Preparação

- [x] 1.1 Criar o crate `sandmux-proto` e o binário `sandmuxd` no pacote `sandmux`, e acrescentar `vt100` (0.18) às dependências do workspace (o daemon usa threads do sistema, sem runtime assíncrono); verificar que `./scripts/check.sh` passa sem mudança nos testes existentes
- [x] 1.2 Extrair para a biblioteca do pacote `sandmux` o preparo do lançamento (verificação pelo `sandmux-run --check`, trava da camada, composição, memfd e comando do `sandmux-run`), recebendo o `Host` e as opções, e fazer o `run` sem terminal usar essa função; verificar que os 126 testes passam sem alteração e que `sandmux run shell -- -c "echo ok" | od -c` mostra só `ok\n`

## 2. Protocolo

- [x] 2.1 Implementar no `sandmux-proto` o enquadramento (tamanho de 4 bytes, tipo, conteúdo), as mensagens de controle em JSON, os quadros de bytes de terminal e o número da versão; verificar com testes de ida e volta, de quadro dividido entre leituras e de quadro acima do limite recusado
- [x] 2.2 Implementar a saudação (número do protocolo, versão, diretório de dados, PID e modo de início) e a recusa por número diferente; verificar com testes de unidade dos dois lados

## 3. Daemon: socket, instância única e controle

- [x] 3.1 Criar a pasta do socket em `$XDG_RUNTIME_DIR/sandmux` (ou `/tmp/sandmux-<uid>`) com 0700, recusando pasta de outro dono, com outro modo ou link simbólico; verificar com testes de cada recusa e do modo criado
- [x] 3.2 Garantir a instância única por `flock` antes de criar o socket e remover o socket órfão de um daemon morto; verificar com teste que um segundo `sandmuxd` termina com erro e o primeiro continua atendendo
- [x] 3.3 Conferir o UID do outro lado com `SO_PEERCRED` em cada conexão e aplicar a saudação; verificar com teste de unidade da decisão de aceitar ou fechar a partir do UID lido (sem privilégio não dá para conectar com outro UID real) e com teste de integração que um cliente com `SANDMUX_PROTOCOL_OVERRIDE` diferente recebe a recusa com as duas versões e a indicação de `sandmux daemon restart`, com o daemon intacto
- [x] 3.4 Recusar na saudação um diretório de dados diferente do daemon, mostrando os dois; verificar com teste de cliente e daemon com `XDG_DATA_HOME` diferentes
- [x] 3.5 Implementar `sandmux daemon status` (rodando, protocolo, PID, modo de início, número de sessões; código diferente de zero sem daemon, sem iniciá-lo) e `sandmux daemon stop`; verificar com testes dos dois estados
- [x] 3.6 Acrescentar à suíte de fuga uma tentativa de conectar, de dentro do sandbox, ao caminho do socket do daemon e a um socket abstrato com o mesmo nome; verificar que as duas falham com o daemon rodando

## 4. Sessões: lançamento e ciclo de vida

- [x] 4.1 Criar a thread de lançamento de vida longa e lançar cada sessão num PTY: `setsid`, `TIOCSCTTY` e `PR_SET_PDEATHSIG` com SIGKILL no filho, `sandmux-run --spec-fd` com a trava herdada; marcar com `CLOEXEC` todos os descritores acima de 2 no filho, liberando só a especificação e a trava, e restaurar máscara e disposições de sinais; verificar com teste que o shell da sessão tem o PTY como terminal de controle, que Ctrl+C interrompe um `sleep` sem matar o shell e que, com duas sessões, `/proc/self/fd` dentro de uma não mostra nenhum descritor além de 0, 1, 2 e a trava
- [x] 4.2 Montar no daemon o `Host` com a pasta, as variáveis seguras, o `SHELL` e os valores de `--pass-env` do pedido, e devolver erros e avisos da composição ao cliente; verificar com testes que `TERM` e a pasta vêm do cliente, que a pasta perigosa é recusada com a mesma mensagem do `run` direto e sem sessão criada, e que o aviso de `--pass-env` chega ao cliente
- [x] 4.3 Implementar o pedido de criar sessão (identificador sem repetição, nome `<harness>:<pasta>`) e `sandmux ls` com estado, código, motivo e número de clientes; verificar com teste dos três estados na listagem e da mensagem sem sessões
- [x] 4.4 Registrar o fim do harness (código do `wait`, inclusive 125 e 127) como sessão encerrada, mantendo o histórico, e avisar os clientes anexados; verificar com teste em que `exit 3` deixa a sessão encerrada (3) e o cliente anexado termina com 3
- [x] 4.5 Implementar `sandmux kill` (SIGTERM, espera de 5 segundos, SIGKILL; clientes desanexados com mensagem; sessão retirada da lista); verificar com teste de um harness que ignora SIGTERM e de que nenhum processo do sandbox sobra no host
- [x] 4.6 Verificar com teste que um SIGKILL no `sandmuxd` com duas sessões vivas deixa zero processos dos dois sandboxes no host

## 5. Saída: filtro, emulador e histórico

- [x] 5.1 Implementar o filtro de OSC (ESC ] e C1, terminadores BEL, ESC \ e C1, sequência dividida entre leituras, descarte acima de 1 MiB) que retira sempre a consulta OSC 52 e retira a gravação sem `--allow-clipboard`; verificar com testes de unidade de cada forma, de divisão em qualquer ponto e de que outras sequências passam iguais
- [x] 5.2 Alimentar um `vt100::Parser` por sessão com o fluxo filtrado e o limite de histórico (padrão 10.000, opção `--history-lines` do `sandmuxd`), e implementar o rastreador dos modos que o `vt100` não acompanha (pilha do teclado do kitty, foco, colagem, mouse); verificar com testes de unidade do rastreador e de que 12.000 linhas deixam as 10.000 mais recentes
- [x] 5.3 Verificar com teste que o texto marcador impresso por uma sessão não aparece em nenhum arquivo do diretório de dados, do `$XDG_RUNTIME_DIR` de teste nem da pasta de estado, inclusive no log do daemon

## 6. Cliente anexado

- [x] 6.1 Criar o auxiliar de teste que roda o cliente num PTY, escreve teclas e lê a tela por um `vt100::Parser`
- [x] 6.2 Implementar o modo anexado: terminal em modo cru, entrada enviada como bytes, saída escrita no terminal, SIGWINCH virando redimensionamento e restauração do terminal ao sair (tela principal, cursor, mouse, foco, colagem, protocolo do kitty, modo de linha); verificar com o auxiliar que, depois de desanexar de uma tela alternativa com mouse ligado, o terminal volta à tela principal e com os modos desligados
- [x] 6.3 Implementar a tecla de prefixo com Ctrl+b como 0x02 e na forma do protocolo do kitty: `d` desanexa, Ctrl+b envia um Ctrl+b na mesma forma, outra tecla é descartada; verificar com testes de unidade das duas formas e com o auxiliar que Ctrl+b d desanexa e a sessão continua viva
- [x] 6.4 Implementar o reenvio ao anexar (histórico formatado, estado da tela, modos do rastreador) com a saída nova enfileirada até o fim do reenvio; verificar com o auxiliar que, ao reanexar a uma sessão shell com 300 linhas, a rolagem tem as 300 linhas e a tela é igual à de um cliente que ficou anexado
- [x] 6.5 Aplicar ao PTY e ao parser o menor tamanho entre os clientes anexados, recalcular ao anexar, desanexar e redimensionar, e manter o último sem clientes; verificar com teste de 200x50 e 100x30 nos dois sentidos
- [x] 6.6 Permitir vários clientes na mesma sessão, com saída para todos e entrada de qualquer um, e implementar `sandmux attach [id]` (única sessão viva sem identificador, erro com a lista quando ambíguo ou inexistente, erro sem terminal) e `sandmux detach <id>`; verificar com testes de dois clientes digitando, de cada erro do `attach` e do `detach` de outro terminal
- [x] 6.7 Tratar o fechamento do terminal (SIGHUP, conexão perdida) como desanexar; verificar com teste que fechar o PTY do cliente deixa a sessão viva com zero clientes

## 7. Run com terminal

- [x] 7.1 Fazer o `sandmux run` criar a sessão e anexar quando a entrada e a saída são terminais, fazendo antes no cliente as verificações de preset, de harness instalado e da importação na primeira execução; verificar com o auxiliar que `run shell` cria a sessão `shell:<pasta>`, que um preset desconhecido não cria sessão e que a pergunta da importação aparece no terminal
- [x] 7.2 Implementar `--allow-clipboard` no `run`; verificar com o auxiliar que uma gravação OSC 52 só chega ao terminal do cliente com a opção e que a consulta nunca chega
- [x] 7.3 Implementar as saídas do `run` no modo sessão (código do harness, zero com a linha de como reanexar ao desanexar, sem mensagem no SIGHUP) e o SIGTERM no cliente como desanexar; verificar com testes de cada caminho
- [x] 7.4 Atualizar o README e a ajuda do `run` com o modo sessão, o modo direto sem terminal, a tecla de prefixo e os comandos de sessão; verificar que `sandmux --help` lista `ls`, `attach`, `detach`, `kill`, `restart` e `daemon`

## 8. Lista de sessões, reinício e restauração

- [x] 8.1 Gravar `sessions.json` (0600, de forma atômica, a cada mudança de estado) com os campos do design e sem valores de variáveis; verificar com teste que o arquivo de uma sessão com `--pass-env` tem o nome e não o valor
- [x] 8.2 Implementar `sandmux restart <id>` para sessão encerrada ou parada, com o ambiente e os valores de `--pass-env` do terminal que pede, recusando sessão viva e variável ausente; verificar com testes dos três cenários da spec
- [x] 8.3 Restaurar ao subir: relançar as sessões vivas sem cliente, deixar paradas, com o motivo, as que têm `--pass-env` e as de pasta inexistente, e manter as encerradas; verificar com teste que para o daemon com três sessões nesses casos, sobe outro e confere `sandmux ls` e o harness relançado
- [x] 8.4 Implementar `sandmux daemon restart` (parar, subir de novo, restaurar); verificar com teste que as sessões voltam e o histórico some

## 9. Início do daemon

- [x] 9.1 Implementar o início direto (`setsid`, entrada e saída em `/dev/null`, log em `$XDG_STATE_HOME/sandmux/sandmuxd.log`, espera de até 5 segundos pelo socket), usado com `SANDMUX_DAEMON_START=direct` ou sem systemd de usuário; verificar com teste que o daemon sobe pelo cliente e continua depois que o cliente termina
- [x] 9.2 Gerar a unidade `sandmuxd.service` (`ExecStart` absoluto, `Restart=on-failure`, `WantedBy=default.target`, `XDG_DATA_HOME` quando definido), instalar, habilitar e iniciar com aviso na primeira vez, e regravar quando o `ExecStart` muda; verificar com teste do conteúdo gerado num HOME falso, sem chamar o `systemctl`
- [x] 9.3 Implementar `sandmux daemon disable` (parar, desabilitar, remover a unidade); verificar com teste da remoção do arquivo num HOME falso, sem chamar o `systemctl`
- [x] 9.4 Mostrar no `sandmux doctor` o estado do daemon, da unidade e o caminho do socket, sem iniciar nada; verificar com teste com o daemon parado
- [x] 9.5 Escrever `docs/sessions.md` com o protocolo, o socket, a lista de sessões, a restauração e os limites do repasse cru; verificar que o teste de documentação a cobre

## 10. Verificação na máquina de referência

- [x] 10.1 Rodar o opencode e o hermes reais em sessões: anexar, desanexar com Ctrl+b d, reanexar de outro terminal, conferir a interface utilizável sem esperar redesenho (inclusive o protocolo de teclado do kitty no opencode) e fechar o terminal com a sessão viva; anotar o resultado no roadmap
- [x] 10.2 Com permissão do usuário, habilitar a unidade systemd real, fazer logout e login, e conferir que o daemon sobe e restaura uma sessão do shell; depois rodar `sandmux daemon disable` e conferir que a unidade sumiu. Feito em 2026-11-13. A máquina tem linger ligado, e com ele o daemon não para no logout. Por isso, no lugar do logout, o teste parou a unidade e subiu o `default.target`, que é o caminho do login. Também matou o daemon com SIGKILL, e o `Restart=on-failure` o trouxe de volta. Nos dois casos a sessão do shell voltou viva e respondeu ao anexar. O `daemon disable` removeu a unidade e o link em `default.target.wants`
- [x] 10.3 Restaurar uma sessão do opencode e do hermes sem cliente anexado (por `sandmux daemon restart`) e conferir que funcionam ao anexar depois; se algum travar por consulta sem resposta, aplicar a resposta fixa prevista nos riscos do design e registrar
- [x] 10.4 Anexar dois clientes a uma sessão do opencode e anotar no roadmap se as respostas duplicadas às consultas do terminal causam algum problema visível
