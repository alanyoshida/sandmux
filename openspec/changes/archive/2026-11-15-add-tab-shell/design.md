## Context

O processo 1 do sandbox desliga o `PR_SET_DUMPABLE`, e depois disso o `/proc` dele não entrega mais os namespaces. Os auxiliares que já existem fazem tudo antes de o supervisor liberar o processo 1:
- o de conexão, das portas do host;
- o de nomes;
- o de montagem, das pastas ao vivo.

Eles nascem antes das threads do supervisor, entram nos namespaces e atendem pedidos por um par de sockets.

## Decisions

- **Auxiliar de entrada (`enter.rs`):**
  - nasce como os outros e só nas sessões do daemon, que têm o socket de controle;
  - abre os namespaces de usuário, pid, mount, ipc, uts e rede do processo 1;
  - entra no de usuário (ganha as capabilities de lá) e no de pid (os filhos nascem dentro);
  - desliga o próprio `PR_SET_DUMPABLE`, porque guarda a especificação.
- **Um pedido** traz dois descritores: a conexão de quem pediu e o terminal. O auxiliar cria um processo de espera, que cria o shell, espera o fim dele e escreve o código de saída, 4 bytes, na conexão.
- **O shell:**
  1. entra nos namespaces de rede, ipc e uts e, por último, no de mount, e com isso a raiz passa a ser a do sandbox;
  2. vira líder de sessão com o terminal como controle, e vai para a pasta inicial;
  3. descarta capabilities, liga `no_new_privs`, aplica o Landlock e o seccomp da especificação e instala o filtro das aberturas;
  4. manda o descritor do filtro ao supervisor, que atende as aberturas dele com o mesmo auxiliar de nomes, agora compartilhado;
  5. executa o `SHELL` da especificação, ou `bash`, ou `sh`, com o ambiente do comando.
  - **Rastreamento:** o shell é descendente do supervisor, e por isso o Yama permite que ele leia a memória do shell para atender as aberturas, como faz com o comando.
- **Terminal de controle:** quem pede (`sandmux-run --enter`) larga o terminal (`TIOCNOTTY`, ignorando o SIGHUP) antes de mandá-lo, para que o shell o tome como controle. Sem isso, o shell fica sem controle de tarefas; um teste confere.
- **Socket:** `<controle>.enter`, no mesmo lugar do socket de controle, com modo 600. `sandmux-run --enter <socket>` manda a entrada padrão (o terminal) por SCM_RIGHTS e sai com o código recebido.
- **Daemon:** a sessão tem o harness `dentro` e o número da aba alvo em `args`.
  - **Lançamento:** pula a composição e lança `sandmux-run --enter` num PTY.
  - **Etiqueta:** usa uma etiqueta própria, para o fim da aba não apagar os arquivos da alvo.
  - **Painel:** mostra a proteção da aba alvo.
  - **Restauração:** depois de um reinício, a aba volta parada.
  - **Reinício (Ctrl+b r):** entra de novo no sandbox atual da alvo.
- **Tetos:** o shell fica no escopo do supervisor da aba alvo e conta nos tetos dela.

- **Teclas:** a validação de `[tui.keys]` só recusa duas escolhidas na mesma tecla. Uma escolhida tira a tecla padrão de outra ação, que aparece com `—` na ajuda e continua no menu. Assim, uma ação nova com uma tecla padrão já usada por alguém não invalida o arquivo dele ao atualizar.

## Risks / Trade-offs

- **[O usuário vê os segredos que o agente não vê?]** Não: o shell passa pelas mesmas regras de arquivos, de propósito, para mostrar exatamente o que o agente vê.
- **[O shell conta como o agente]** As aberturas e a rede do shell entram nos registros da aba alvo. É o comportamento esperado de um shell dentro do sandbox, e está documentado.
