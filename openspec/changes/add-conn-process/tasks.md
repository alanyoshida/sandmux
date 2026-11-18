Os testes com o Docker real, o `./scripts/check.sh` e o `scripts/check-root.sh` rodam sob o vigia de memória, com `CARGO_BUILD_JOBS=16`. Os testes com root precisam da linha do `sudoers` de `scripts/check-root.sh`.

## 1. Programa de conexões (com root primeiro)

- [ ] 1.1 Montar o programa `tp_btf` de `inet_sock_set_state` (o filtro do cgroup, o `SYN_SENT`, o evento `EV_CONN` com o pid, a família, as portas e o destino) e carregá-lo na observação junto com o de execução. Verificar com testes de montagem e de leitura do evento, e com um teste com root em que o `observe` de um escopo recebe a conexão de um `curl` ao `127.0.0.1` com a porta de origem certa

## 2. Daemon

- [ ] 2.1 Guardar o mapa de conexões por sessão (o executável pelo `/proc/<pid>/exe`, até 4096) e responder ao `ConnProcess`, com a segunda tentativa nas abas pelo kernel. Subir a versão do protocolo. Verificar com testes unitários do mapa e da resposta

## 3. Gateway

- [ ] 3.1 Na hora de gravar a linha (TCP e UDP), perguntar ao daemon e cair na busca pelo `/proc` (`net/tcp*`, `net/udp*`, os processos do namespace de pid do sandbox, `exe`). Acrescentar `pid`, `exe` e `proc_via` ao registro. Verificar com testes unitários do leitor de `/proc/net/tcp` e com um teste de `sandmux run` sem o auxiliar em que uma conexão a um servidor local tem o processo com `proc_via` = `proc`
- [ ] 3.2 Mostrar o processo no `sandmux net` e na tela de rede da TUI. Verificar com os testes de `net` e da TUI

## 4. Verificações com root

- [ ] 4.1 Teste com o auxiliar real: numa aba, a conexão de um `curl` a um servidor local tem o pid e o executável do `curl` com `proc_via` = `kernel`; uma conexão de um contêiner do Docker não tem processo
- [ ] 4.2 Medir o custo: 200 conexões a um servidor local numa aba, com e sem o auxiliar, e anotar no roadmap

## 5. Documentação e fechamento

- [ ] 5.1 Atualizar `docs/network.md` (os campos novos, as fontes, os limites, o Docker sem processo), `docs/helper.md`, `docs/sessions.md` (o protocolo) e os pares em inglês, com os testes de `docs.rs`
- [ ] 5.2 Rodar `./scripts/check.sh` e `scripts/check-root.sh` inteiros sob o vigia

## Workflow follow-up

- Commit e push num branch novo, PR, merge e release nova (ordem do usuário, 2026-11-17).
- Depois: o change 74 (Ubuntu 24.04), pedido do usuário.
