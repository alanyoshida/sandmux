## Why

O caso aberto do change 79 (o lançamento recusado logo depois de um reinício do auxiliar root, no 6.8) ficou mais frequente nos testes com root na VM em 2026-11-21: 3 falhas em 3 de 4 rodadas. A investigação de 2026-11-22, a pedido do usuário ("Sim, investigar"), achou a causa, e ela não é a carga dos programas do escopo:

- Medido na VM (6.8): o auxiliar novo responde ao primeiro `status` em 0,5 s depois de subir, com a carga dos programas.
- A falha vem da verificação antes do lançamento (`support::check`, o pedido `status` ao auxiliar). Ela faz uma tentativa só. O registro do escopo, logo depois, já tenta de novo por até 30 s.
- Na saída limpa (`SIGTERM`), o auxiliar velho fecha cada conexão nova sem responder, como a spec pede. Isso dura de 0,5 a 3,7 s. Reproduzido na VM: um `status` nesse intervalo recebe `EPIPE` no envio ou `ECONNRESET` (ou o fim da conexão) na leitura. O lançador traduz isso por "o auxiliar root não respondeu ao pedido" e recusa o lançamento na hora.
- Os testes que falharam lançam uma aba logo depois de pedir o reinício: `escopo_de_volta_depois_do_reinicio` e `execucoes_pelo_kernel_e_recuo` (o primeiro lançamento depois do reinício no fim do teste anterior).

O roadmap dizia que era o caminho dos 30 s sem resposta. Estava errado: a mensagem é a mesma, mas a recusa é imediata.

O mesmo vale fora dos testes: um `systemctl restart sandmux-helper.service` (o `sandmux setup` e o `install.sh` fazem isso desde o change 88) recusa um lançamento feito nesse intervalo.

## What Changes

- **A verificação do auxiliar antes do lançamento tenta de novo, como o registro:** enquanto o auxiliar não responde (a conexão fechada sem resposta, o envio recusado, o prazo de leitura), por até 30 s; sem o socket, por até 1 s seguido. Uma resposta (com ou sem o suporte ao escopo) e uma recusa com motivo não se repetem.
- **Teste:** um auxiliar falso que fecha as primeiras conexões sem responder, como o velho na saída, e responde à seguinte. A verificação passa com a correção e falha com o código da 0.1.36.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `root-helper`: o requisito "Saída limpa do auxiliar" passa a valer também para a verificação do auxiliar antes do lançamento, e não só para o registro do escopo.

## Impact

- `sandmux-launcher/src/scope.rs`: o `status` pelo `retry`, e o teste.
- Numa máquina com o Landlock abaixo do ABI 6 e sem o auxiliar instalado, a recusa do lançamento passa a levar 1 s, o prazo do socket que falta (antes, era imediata). Continua abaixo dos 2 s do cenário da spec.
- A spec `root-helper` e o roadmap (o caso aberto do change 79 corrigido).
