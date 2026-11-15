## 1. Supervisor

- [x] 1.1 Alvo `IdMap` para `uid_map`, `gid_map`, `projid_map` e `setgroups` do próprio processo, aberto por um auxiliar no user namespace pai (`NS_GET_PARENT`, `setns`, `SCM_RIGHTS`); recusa quando o pai é o namespace do supervisor
- [x] 1.2 Tradução do `/proc/self` pelo pid no pid namespace do sandbox (segundo número do `NSpid`)

## 2. Testes e documentação

- [x] 2.1 Testes: tradução dos alvos; `unshare -r` no sandbox; `/proc/self/status` num pid namespace aninhado
- [x] 2.2 Verificação manual: o Chrome headless e com janela, sem `--no-sandbox`, com os renderers em pid namespaces aninhados e seccomp
- [x] 2.3 Documentação: `docs/window.md` e o par em inglês
