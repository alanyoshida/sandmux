## Context

Veja a proposta para o motivo. O que pesa na escolha:

- Cada teste de integração sobe o seu próprio daemon e sandbox, numa pasta temporária própria (`Env::new`), sem porta fixa nem caminho fixo fora dela. Os contadores internos de `network` e `docker` já levam o PID no nome. Não há doctests.
- A máquina de referência já travou por falta de memória com um teste do Docker (2026-11-14). Toda rodada pesada passa pelo vigia, que mata a rodada abaixo de 6 GB disponíveis.
- Com o `cargo test`, os 15 testes de `tests/docker.rs` já rodavam todos juntos, nas threads do próprio arquivo.

## Goals / Non-Goals

**Goals:**
- Os arquivos de teste em paralelo, sem passar do pico de memória de hoje.
- O `check.sh` funcionando igual em quem não tem o nextest.

**Non-Goals:**
- Tornar o nextest obrigatório ou levá-lo a um CI de testes (o repositório só tem o workflow de release).
- Encurtar os testes longos em si (o mais longo, `kernel_antigo_sem_o_auxiliar_recusa_sem_executar`, leva 30 s e passa a ser o piso).
- Os testes com root do `scripts/check-root.sh`, que seguem pelo `cargo test`.

## Decisions

- **cargo-nextest, opcional:** cada teste num processo, em paralelo entre todos os binários, com grupos e prioridades por filtro. O `check.sh` usa o nextest quando `cargo nextest --version` responde, e o `cargo test` senão. Alternativa descartada: rodar vários `cargo test --test <arquivo>` ao mesmo tempo no script, que deixaria o tempo preso a grupos montados à mão.
- **Medição (2026-11-21, build já compilado, sob o vigia):**

  | Rodada | Tempo de parede | Queda da memória disponível |
  |---|---|---|
  | `cargo test --workspace` (hoje) | 179 s | 1,26 GB |
  | nextest, 16 testes simultâneos, Docker um por vez | 55 s | 1,42 GB |
  | nextest, 28 testes simultâneos, Docker um por vez | 56 s | 1,57 GB |
  | nextest, 16, Docker um por vez, arquivos longos primeiro | 52 s | 1,51 GB |
  | nextest, 16, Docker até 4, arquivos longos primeiro (duas rodadas) | 31 s e 31 s | 1,13 GB e 1,42 GB |

  Todas com 814 testes passando, 4 pulados (os mesmos 4 ignorados do `cargo test`), e nenhum processo de teste vivo depois.
- **16 testes simultâneos:** 28 (o número de CPUs) não foi mais rápido e usou mais memória.
- **Docker até 4 de uma vez:** com um por vez, os testes do Docker somam 51 s em série e viram o caminho mais longo. Com até 4, a rodada cai para o piso do teste mais longo, com a memória dentro do que o `cargo test` já usava, que rodava os 15 juntos.
- **Os arquivos longos primeiro** (`escape_launcher`, `gateway`, `history`, `db_mcp`, e o Docker logo depois): pela prioridade do nextest, para o teste de 30 s não começar no fim.
- **`fail-fast = false`:** a rodada mostra todas as falhas, e não só a primeira.
- **Vazamento achado pelo nextest:** numa das rodadas, `hermes_config::tests::pid_do_gateway_do_host` saiu como `LEAK` (um processo do teste segurava a saída depois do fim). O `sleep` era neto do `sh` que o teste matava. Agora o teste sobe o `sleep` direto, com `gateway` no `argv[0]` (o que a função testada procura), e o kill o alcança.
- **`slow-timeout = "60s"`:** avisa de um teste lento a cada minuto sem matá-lo, porque há testes do Docker que esperam até 3 min por um contêiner.

## Risks / Trade-offs

- **[Pico de memória numa máquina ocupada]** As rodadas foram medidas com 17 a 18 GB disponíveis no começo. → O vigia continua valendo; se ele matar uma rodada, os tetos (`test-threads` e o do grupo `docker`) baixam no `.config/nextest.toml`.
- **[Teste que dependia de rodar no mesmo processo que outro]** Um teste que contasse com estado deixado por outro do mesmo arquivo quebraria. → Nenhum quebrou em três rodadas completas; os contadores internos já usam o PID.
- **[Duas formas de rodar]** Quem não tem o nextest segue no `cargo test`, que passa a ser o caminho menos usado. → O `check.sh` diz como instalar o nextest, e os dois rodam os mesmos 814 testes.
