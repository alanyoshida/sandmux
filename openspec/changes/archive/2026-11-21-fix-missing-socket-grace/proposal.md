## Why

O change 90 fez o lançamento desistir em 1 s quando o socket do auxiliar root não existe. Ao rodar os testes com root na VM (2026-11-21, a pedido do usuário), a análise da troca do auxiliar mostrou uma falha nele: o prazo de 1 s contava desde a primeira tentativa, e não desde que o socket sumiu. Numa troca, o auxiliar velho demora segundos a sair (sem responder), e só depois o socket some por um instante, até o novo criá-lo. Se uma tentativa caísse nesse instante, já passado 1 s do começo, o lançamento era recusado na hora, em vez de esperar o auxiliar novo. O próprio change 90 dizia que o prazo era para cobrir a troca.

A falha na VM (`escopo_de_volta_depois_do_reinicio`, no 6.8) não foi essa: a mensagem é a do auxiliar que não respondeu por 30 s, o caso já aberto no roadmap desde o change 79. A análise dela é que levou a esta.

## What Changes

- **O prazo curto conta da primeira falta seguida do socket:** o lançamento desiste sem o socket só quando ele falta por 1 s seguido. Qualquer outra resposta (o auxiliar que não responde, ou que responde) zera a conta. O prazo longo de 30 s continua contando do começo.
- **Teste:** o teste de unidade do `retry` ganha o caso da troca (o auxiliar velho sem responder por mais que o prazo curto, o socket faltando um instante, o novo respondendo). Ele falha com o código da 0.1.35 e passa com a correção.
- O requisito da spec não muda: "quando o socket do auxiliar não existe, o lançamento MUST desistir em até 1 s" continua valendo, e a troca segue coberta pelos 30 s.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

(nenhuma: a correção faz o código cumprir o requisito do change 90)

## Impact

- `sandmux-launcher/src/scope.rs`: o `retry` e o teste dele.
- O roadmap.
