## Why

Na VM do Ubuntu 24.04 (change 74), `porta_do_sandbox_publicada_no_host` e `allowlist_de_portas_endereco_e_registro` falharam em três rodadas, nos kernels 6.8 e HWE, sempre com a VM ocupada (logo depois do boot, com o cloud-init, o apt e o snapd rodando, ou depois de instalar pacotes). Depois, na VM ociosa, a suíte de rede passou 17 rodadas seguidas, também com a CPU carregada, e uma porta do sandbox foi publicada em cerca de 1,1 s. Não há caminho de código que falhe só no 24.04.

Os dois testes dependem de prazos curtos: esperam a publicação por 4 ou 5 s, e o servidor do sandbox vive só 4 s (`sleep 4; kill`) ou 8 s. Numa máquina ocupada, o lançamento e a primeira varredura das portas (a cada 1 s) podem passar disso. Quando falham, os testes não dizem o que foi publicado.

Decisão do usuário (2026-11-20): uma correção pequena nos testes, sem release nova.

## What Changes

- **Diagnóstico:** quando a publicação não acontece, os dois testes mostram a lista de portas do lançamento (`run-<pid>.ports.json`) e a resposta do host.
- **Prazos:** a espera pela publicação sobe para 10 s. O servidor do sandbox não morre mais por tempo: o script espera um arquivo de parada na pasta de trabalho, que o teste cria depois das conferências. Assim, a conferência de que a porta fecha continua valendo, sem depender de quanto tempo o lançamento levou.
- Nenhuma mudança no programa nem nas specs.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

(nenhuma: o change só muda testes)

## Impact

- `crates/sandmux/tests/network.rs`: os dois testes de publicação de portas.
- `openspec/roadmap.md`: o achado do change 74 fecha como não reproduzido, com a causa provável nos prazos dos testes.
