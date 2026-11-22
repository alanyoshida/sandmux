## Why

O `./scripts/check.sh` roda os 68 arquivos de teste do workspace um depois do outro. O `cargo test` paraleliza os testes de dentro de cada arquivo, mas não os arquivos entre si: na rodada de 2026-11-21, só a execução dos testes somou uns 170 s, e cinco arquivos (`escape_launcher`, `docker`, `gateway`, `db_mcp` e `history`) somaram 94 s. O usuário perguntou se dava para paralelizar e escolheu o cargo-nextest (2026-11-21).

## What Changes

- **cargo-nextest no `check.sh`:** quando `cargo nextest` está instalado, o passo de testes roda `cargo nextest run --workspace`, que executa cada teste como um processo próprio, em paralelo entre todos os arquivos. Sem o nextest, o `check.sh` segue com o `cargo test --workspace` de hoje e diz como instalar o nextest.
- **Configuração em `.config/nextest.toml`:** o perfil padrão limita os testes simultâneos, põe os testes do Docker privado num grupo com teto próprio, pelo pico de memória (o histórico de travamento da máquina com testes do Docker), e começa pelos arquivos mais longos. Os valores saem de uma medição de tempo e de memória antes e depois, registrada no design.
- **Documentação:** como rodar a verificação com e sem o nextest, no `README.md` e no `README.en.md`, onde o `check.sh` já é descrito.
- **Um teste que vazava um processo:** `hermes_config::tests::pid_do_gateway_do_host` subia um `sh` com um `sleep` neto; o kill no fim matava só o `sh`, e o `sleep` sobrava até 5 s com a saída do teste aberta. O nextest marca isso como `LEAK`. O teste passa a subir o próprio `sleep`, com `gateway` no `argv[0]`.
- Nenhuma mudança no programa nem nas specs.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

(nenhuma: o change só muda a forma de rodar os testes)

## Impact

- `scripts/check.sh` e o novo `.config/nextest.toml`.
- Ferramenta de desenvolvimento nova e opcional: `cargo install cargo-nextest --locked`. O workflow de release não roda testes e não muda.
- `README.md`, `README.en.md` e o roadmap.
