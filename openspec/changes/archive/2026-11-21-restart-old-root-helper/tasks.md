Os testes rodam no host, sob o vigia de memória, com `CARGO_BUILD_JOBS=16`. Nenhum programa BPF é carregado, e nada reinicia o auxiliar root real: os testes usam um auxiliar falso e funções puras.

## 1. Árvore de processos

- [x] 1.1 Em `daemon/proctree.rs`, `changes`: sem o `ppid` no evento (ou com 0), usar o pai do `/proc/<pid>/stat`, que já é lido para o `starttime`. Verificar com um teste de unidade num `/proc` falso: `bash` filho do `opencode`, `sleep` sem `ppid` no evento sai filho do `bash`; um pid que não existe no `/proc` fica sem pai

## 2. Reinício do auxiliar antigo

- [x] 2.1 Em `setup.rs`: o passo `Restart` de `helper_step`, quando as unidades estão em dia (ou vêm de um pacote) e o auxiliar em execução é mais antigo; reiniciar com `systemctl try-restart`, como root direto ou com `sudo` depois de perguntar; com `--no-sudo` ou sem terminal, mostrar o comando. Verificar com testes de unidade do `helper_step`
- [x] 2.2 Em `packaging/install.sh`: com privilégio e as unidades em `/etc/systemd/system`, `systemctl try-restart sandmux-helper.service` depois de gravar os binários. Verificar com o teste de `docs.rs` que confere o `install.sh` e com `sh -n`
- [x] 2.3 O texto do aviso no `doctor` e na seção Processos, com `sandmux setup` e `sudo systemctl restart sandmux-helper.service`, sem a frase da árvore sem pais. Verificar com os testes do aviso em `tests/cli.rs` e `tests/tui.rs`, ajustados ao texto novo

## 3. Documentação e fechamento

- [x] 3.1 `docs/helper.md`, `docs/install.md`, `docs/tui.md` e as versões em inglês: o reinício do auxiliar numa atualização, o comando manual e a árvore com o pai pelo `/proc`. Verificar com os testes de `docs.rs`
- [x] 3.2 Rodar o `./scripts/check.sh` inteiro no host e conferir que termina verde e sem processo de teste vivo
- [x] 3.3 Atualizar o roadmap com o change e o achado

## Workflow follow-up

- Commit e push num branch novo, PR e merge; arquivar o change e sincronizar as specs no mesmo PR.
- Release v0.1.34 com este change e o 85 (grafo do cofre), a pedido do usuário ("Sim, corrigir tudo").
