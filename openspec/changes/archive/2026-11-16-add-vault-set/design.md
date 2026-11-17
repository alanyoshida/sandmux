## Context

- O cofre guarda cada segredo com o nome, o valor, o falso, os domínios e as ferramentas (`sandmux-vault`, `VaultContents::put`). O `put` substitui pelo nome e gera um falso novo quando a mensagem não traz um.
- `Msg::VaultSetDomains` (protocolo 24) já troca os domínios sem pedir o valor de novo, com a tecla `d` na TUI.
- O gateway pede os segredos da aba ao daemon a cada conexão TLS decifrada (`inject::fetch` em `sandmux-net/src/lib.rs`).

## Goals / Non-Goals

**Goals:**
- Trocar só o valor de um segredo de token, sem tocar no falso, nos domínios nem nas ferramentas.

**Non-Goals:**
- Trocar uma chave SSH (`vault add-ssh` de novo) ou uma conexão de banco (`sandmux db add`).
- Mudar o comportamento do `vault add` (decisão do usuário).
- Avisar quando uma chave vence.

## Decisions

### Mensagem própria
`Msg::VaultSetValue { name, value }` em vez de reaproveitar o `VaultAdd` com o falso antigo. O cliente não precisaria conhecer o falso, mas teria de repetir os domínios e as ferramentas, e uma corrida com outra mudança poderia perdê-los. O daemon troca o valor no lugar, com o cofre aberto, e grava.

Não muda nada que a política de rede usa (os domínios são os mesmos), então não há recompilação das políticas.

### Recusas
- **Nome que não existe:** "não existe o segredo X no cofre; para criar, use `sandmux vault add X`".
- **Chave SSH:** aponta o `vault add-ssh`.
- **Conexão de banco:** aponta o `sandmux db add`.
- **Valor vazio:** recusado no cliente, como no `vault add`.

### TUI
`FormKind::Value(nome)` com um campo mascarado ("Valor novo"), enviado como `VaultSetValue`. O texto é apagado da memória da TUI depois do envio, e a mensagem com o valor também, como no `Add`. A tecla `v` só abre o formulário com um segredo de token escolhido; numa chave SSH, ela avisa e não faz nada.

## Risks / Trade-offs

- [Conexões já abertas do harness seguem com o valor antigo até fechar] → É o mesmo comportamento da troca de domínios; o texto da ajuda diz que vale na próxima conexão, e Ctrl+b `r` força.
