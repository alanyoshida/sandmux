## Why

O ciclo do change 74 deixou três pontas soltas, todas ligadas ao incidente do BPF de 2026-11-20 e ao que a VM do Ubuntu 24.04 mostrou:

- **O `O_PATH` no musl, no Landlock:** o `PathFd` do crate `landlock` usa `OpenOptions::custom_flags(O_PATH)`. No musl, o `O_ACCMODE` inclui o `O_PATH`, e o Rust o tira. É o mesmo bug que parava o Docker confinado. Nos binários das releases, o lançador abre cada caminho das regras do Landlock para leitura:
  - um caminho que o usuário não lê é pulado sem aviso e fica sem acesso;
  - um FIFO na allowlist (um `--ro` ou `--rw` que aponta para um pipe com nome) trava o lançador no `open`.
- **Um teste de TUI instável:** `rede_no_painel_e_marca_de_dominio_novo_na_barra` falha de vez em quando e já parou um `check.sh`. O painel mostra duas linhas do domínio, a do DNS e a da conexão, e só a primeira gravada ganha o "novo". Quando é a do DNS, o "novo" dela cai fora da tela de 30 linhas do teste.
- **Os testes com root no host:** carregar programas BPF novos na máquina do usuário a derrubou duas vezes. A VM do 24.04 já roda esses testes, mas por um roteiro de trabalho fora do repositório. Dois testes do Docker por eBPF dependem do reinício do auxiliar pelo arquivo `.restart`, que só o `scripts/check-root.sh` faz.

Decisão do usuário (2026-11-20): a VM passa a ser o padrão para os testes com root, e o `scripts/check-root.sh` avisa e pede confirmação antes de carregar BPF no host.

## What Changes

- **Lançador:** as regras do Landlock são abertas pelo `open` direto, com `O_PATH | O_CLOEXEC`, no lugar do `PathFd`. Um caminho sem leitura e um FIFO passam a ter a regra, sem abrir o conteúdo.
- **Teste de TUI:** o teste do painel de rede usa um terminal mais alto, com espaço para as duas linhas do domínio.
- **`scripts/vm-ubuntu-2404.sh check-root`:** compila os testes com root e o auxiliar estáticos (musl), copia para a VM no mesmo caminho e roda os testes com root lá dentro:
  - o auxiliar roda como root na VM;
  - os testes rodam como o usuário, sob o systemd de usuário;
  - o reinício pelo `.restart` funciona como no `check-root.sh`.

  Um passo `docker` prepara o Docker sem root na VM, com o `rootlesskit` do repositório do Docker.
- **`scripts/check-root.sh`:** avisa que carrega BPF no kernel do host e pede confirmação. Uma variável a dispensa, para quem já decidiu.
- **Documentação:** `docs/helper.md` (os testes) e os pares em inglês.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `sandbox-filesystem`: um caminho acrescentado pelo usuário que é um FIFO, ou que o usuário não lê, não trava nem some do lançamento.

## Impact

- `crates/sandmux-launcher/src/landlock_rules.rs`.
- `crates/sandmux/tests/tui.rs`.
- `scripts/vm-ubuntu-2404.sh` e `scripts/check-root.sh`.
- `docs/helper.md`, `docs/install.md` e os pares em inglês.
