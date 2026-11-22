## Context

O lançamento num kernel com o Landlock abaixo do ABI 6 fala com o auxiliar root duas vezes. Primeiro, na verificação da máquina (`support::check`, no processo de sondagem do `sandmux-run`), manda o pedido `status`. Depois, antes da liberação do filho, manda o pedido `scope`. Só o segundo passava pelo `retry` do change 79 (30 s sem resposta, 1 s sem o socket, changes 90 e 91). Na saída limpa, o auxiliar fecha toda conexão nova sem responder (`accept_loop`, com o `closing` ligado). Um `status` nesse intervalo falhava na hora, e o lançamento era recusado com "esta máquina não permite lançar o sandbox".

## Goals / Non-Goals

**Goals:**
- A verificação antes do lançamento atravessa a troca de auxiliar como o registro já atravessa.
- Um teste que reproduz o auxiliar na saída e falha com o código de antes.

**Non-Goals:**
- Mudar a saída limpa do auxiliar. Ela continua sem atender conexões novas, como a spec pede.
- Mudar o `sandmux doctor`. Ele tem o próprio cliente e mostra o estado do momento.

## Decisions

- **Corrigir no cliente, e não no auxiliar.** Uma alternativa era o auxiliar parar de aceitar na saída, deixando as conexões na fila do socket para o auxiliar seguinte. Isso só ajuda com o socket do systemd. Quando o socket é recriado (o `--socket` dos testes e da VM), a fila morre com o processo velho. Também reabriria a corrida do change 79 (aceitar e cortar). O contrato já decidido é que quem pede tenta de novo; faltava a verificação seguir o mesmo contrato.
- **Os mesmos prazos do registro** (`RETRY_FOR` de 30 s, `MISSING_FOR` de 1 s, a cada 300 ms), pelo mesmo `retry`. O envio recusado (`EPIPE`) passa a ser "sem resposta", como no registro. O auxiliar que responde sem o suporte ao escopo e a resposta com erro não se repetem.
- **O teste fica no lançador, com um socket de verdade.** O host tem o 7.0 (ABI 6 ou mais), e o caminho do auxiliar só roda abaixo do ABI 6. Um teste de unidade com um servidor `SOCK_SEQPACKET` que fecha as primeiras conexões reproduz o que a VM mostrou. Para não mexer na variável de ambiente dentro do processo de testes, o `status` ganha uma forma com o caminho do socket.

## Risks / Trade-offs

- [Um auxiliar instalado mas travado faz o lançamento esperar 30 s antes da recusa, e não 10 s] → É o mesmo prazo do registro, que já esperava 30 s nesse caso.
- [Sem o auxiliar instalado, a recusa leva 1 s, e não é imediata] → Dentro dos 2 s do cenário da spec; o teste `kernel_antigo_sem_o_auxiliar_recusa_sem_executar` confere menos de 10 s.
