# Proposal

## Why

Quando o vigia de segredos (change 46) move uma chave real para o cofre, a aba viva que ainda usa a configuração antiga precisa ser reiniciada para passar a ler o valor falso e deixar o gateway trocá-lo pelo real no tráfego. Hoje o único aviso garantido disso é a notificação de desktop (change 48), que é suprimida justamente quando a TUI está em foco — o caso mais comum, já que o usuário costuma estar olhando a própria aba quando configura a chave. O resto do aviso (texto no `sandmux ls` e na seção Diagnóstico) é passivo: só aparece para quem já sabe que precisa procurar. Isso levou a um caso real em que o usuário configurou a chave do OpenRouter no opencode, recebeu "Missing Authentication header" e não tinha pista do motivo.

## What Changes

- A TUI passa a mostrar um modal quando uma ou mais sessões vivas passam a precisar de reinício por causa de uma chave movida ao cofre, independente de qual aba está ativa no momento.
- O modal lista cada sessão viva afetada (identificador, nome, harness); `r`/Enter reinicia a sessão selecionada (reinício forçado, como o Ctrl+b `r` já faz), uma tecla reinicia todas as listadas, e Esc fecha o modal sem reiniciar nada.
- O modal reaparece quando uma sessão nova entra nessa condição depois de fechado (uma chave movida mais tarde, numa sessão ainda não vista nessa leva), mesmo que o usuário já tenha dispensado o aviso antes.
- Não substitui a notificação de desktop do change 48 nem o texto do `sandmux ls`/Diagnóstico — soma-se a eles.
- Não muda o comportamento do vigia de segredos: continua sem reiniciar nada sozinho (decisão do change 46).

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `terminal-ui`: novo requisito para o modal de aviso de chaves movidas, independente da aba ativa, usando o contador de chaves movidas por sessão que o daemon já expõe (change 46) e o reinício forçado que o Ctrl+b `r` já usa (change 61).

## Impact

- Crate `sandmux` (TUI): novo estado no `App` para acompanhar, por sessão, até onde o cliente já viu o contador de chaves movidas, e o modal novo (desenho, teclas, disparo).
- Nenhuma mudança no daemon, no protocolo nem no cofre: o modal só lê o campo já existente na lista de sessões e reaproveita o pedido de reinício forçado que já existe.
- Nenhuma mudança em `sandmux ls`, na notificação de desktop ou na seção Diagnóstico.
