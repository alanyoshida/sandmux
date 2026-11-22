As medições rodam no host, sob o vigia de memória, com `CARGO_BUILD_JOBS=16`, com o build já compilado. Nenhum programa BPF é carregado.

## 1. Medição e configuração

- [x] 1.1 Instalar o cargo-nextest (`cargo install cargo-nextest --locked`, 0.9.152) e medir o `cargo test --workspace` de hoje: tempo de parede e queda da memória disponível. Verificar que os 814 testes passam
- [x] 1.2 Escrever o `.config/nextest.toml` (teto de testes simultâneos, grupo `docker`, prioridade dos arquivos longos, `fail-fast` e `slow-timeout`) e medir com o nextest, variando o teto e o grupo. Verificar que os 814 testes passam, sem processo de teste vivo, e registrar a tabela no design

## 2. Script e documentação

- [x] 2.1 `scripts/check.sh` usa o nextest quando instalado e o `cargo test` senão, dizendo como instalar. Verificar com `sh -n` e rodando o script
- [x] 2.2 `README.md` e `README.en.md`, onde o `check.sh` é descrito. Verificar com os testes de `docs.rs`

- [x] 2.3 `hermes_config::tests::pid_do_gateway_do_host` sem o `sleep` neto que o nextest marcou como `LEAK`. Verificar rodando o teste seis vezes pelo nextest, sem `LEAK`

## 3. Verificação e fechamento

- [x] 3.1 Rodar o `./scripts/check.sh` inteiro com o nextest e conferir que termina verde e sem processo de teste vivo
- [x] 3.2 Atualizar o roadmap com o change e a medição

## Workflow follow-up

- Commit e push num branch novo, PR e merge; arquivar o change no mesmo PR.
- Sem release nova: o programa não muda.
