# Proposal

## Why

Hoje um agente confinado vive enquanto vive o terminal que rodou `sandmux run`: fechar a janela, perder a conexão SSH ou reiniciar a máquina mata o harness no meio do trabalho. O sandmux precisa de um processo de longa duração que segure os agentes, como o servidor do tmux. Esse mesmo processo é onde as próximas etapas vão pôr a política, o registro de acessos, o gateway de rede e o cofre. Esta é a etapa 2 do roadmap, e a TUI da etapa 3 se apoia nela.

## What Changes

- Novo binário `sandmuxd`, o daemon de sessões, um por usuário, ouvindo num socket Unix privado em `$XDG_RUNTIME_DIR`. Só o próprio usuário se conecta, e o sandbox não alcança o socket.
- Cada sessão é um harness (ou o shell) lançado pelo `sandmux-run` dentro de um PTY que o daemon segura. As sessões sobrevivem ao cliente. Existe um conjunto único de sessões por usuário, sem sessões nomeadas.
- Comandos novos no cliente: `sandmux ls`, `attach`, `detach`, `kill` e `restart`, mais `sandmux daemon status|stop|restart`.
- **BREAKING**: com terminal interativo, `sandmux run <harness>` passa a criar uma sessão no daemon e anexar o terminal a ela. Fechar o terminal desanexa em vez de encerrar o agente. Sem terminal (script, pipe, CI), o `run` continua rodando o sandbox direto em primeiro plano, sem daemon, com a saída limpa e o código de saída do harness.
- Anexar sem TUI: o terminal do usuário vira o terminal da sessão. Ao anexar, o histórico é reenviado e a tela atual é redesenhada. Ctrl+b d desanexa e Ctrl+b Ctrl+b envia um Ctrl+b ao harness. Vários clientes podem se anexar à mesma sessão e todos interagem. O tamanho da tela segue o menor terminal anexado.
- Histórico de 10.000 linhas por sessão, só em memória, ajustável por opção do daemon. Nada da saída do agente vai para o disco.
- Sessão encerrada continua na lista com o código de saída e pode ser reiniciada na mesma pasta e com as mesmas opções.
- Filtro de sequências de escape na saída da sessão: a escrita no clipboard (OSC 52) é bloqueada por padrão e liberada por sessão com `sandmux run --allow-clipboard`.
- Início do daemon: uma unidade systemd de usuário, instalada e habilitada pelo cliente na primeira vez em que ele precisa do daemon, sobe o daemon no login. Sem systemd, o cliente sobe o daemon na hora.
- Restauração: a lista de sessões, sem nenhum valor de `--pass-env`, é guardada no diretório de dados. Depois de um reboot ou de `sandmux daemon restart`, o daemon relança cada sessão viva na mesma pasta e com as mesmas opções. As que usavam `--pass-env` ficam paradas até o usuário reiniciá-las de um terminal que tenha as variáveis.
- Protocolo versionado entre cliente e daemon. Com versões diferentes, o cliente recusa, explica e indica `sandmux daemon restart`. Nada reinicia sem o usuário pedir.

## Capabilities

### New Capabilities

- `session-daemon`: o processo `sandmuxd`. Como ele começa a rodar (systemd de usuário ou o próprio cliente), o socket e quem pode se conectar, a instância única por usuário, a verificação de versão do protocolo, a parada e o reinício, e a restauração das sessões depois de um reboot.
- `terminal-sessions`: as sessões que o daemon segura. Criar, listar, anexar, desanexar, encerrar e reiniciar; o PTY de cada sessão; vários clientes na mesma sessão e o tamanho da tela; o histórico em memória; o filtro de sequências de escape; o que acontece quando o harness termina.

### Modified Capabilities

- `sandbox-isolation`: o requisito "Terminal do usuário entregue ao comando" e o requisito "Ciclo de vida e código de saída" passam a distinguir o `run` com terminal, que cria uma sessão no daemon e sobrevive ao terminal, do `run` sem terminal, que continua direto em primeiro plano.

## Impact

- **Código**:
  - binário novo `sandmuxd`;
  - crate novo para o protocolo entre cliente e daemon;
  - o cliente `sandmux` ganha os comandos de sessão e o modo anexado;
  - o lançamento (composição da especificação, trava da camada, memfd e `sandmux-run`) passa a ser feito também pelo daemon, a partir das opções e do ambiente que o cliente envia.
- **Dependências novas**: um emulador de terminal em memória para o histórico e o redesenho (a crate `vt100`) O daemon usa threads do sistema, sem runtime assíncrono.
- **Sistema do usuário**: o cliente grava `~/.config/systemd/user/sandmuxd.service` e a habilita com `systemctl --user`, avisando quando faz isso. `sandmux daemon disable` remove a unidade.
- **Dados**: arquivo novo com a lista de sessões no diretório de dados do sandmux, sem valores de variáveis.
- **Compatibilidade**: scripts que chamam `sandmux run` sem terminal não mudam. Quem chama com terminal passa a ter sessões persistentes. O `sandmux-run` e a especificação de lançamento não mudam.
- **Fora do escopo**: a TUI com abas (etapa 3), a configuração em TOML e os perfis (change 3), os limites de recurso (change 4), o histórico em disco (depois da v1) e as sessões nomeadas (depois da v1).
