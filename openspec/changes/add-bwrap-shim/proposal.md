## Why

Harnesses de agente isolam os próprios comandos com o bubblewrap. O Claude Code no Linux usa o `bwrap` no sandbox do bash dele (ligado pelo `/sandbox`). Dentro do sandmux, o bubblewrap falha: ele precisa montar, e o sandbox nega montagens (o Landlock com regras de arquivo e o seccomp). O usuário quer o Claude Code e o opencode funcionando por padrão, com segurança.

Decisão do usuário (2026-11-20), depois de comparar com o modo de montagens:

- não tirar o Landlock de nenhuma aba. O modo de montagens (bubblewrap e podman de verdade) e o suporte ao podman ficam adiados;
- um `bwrap` substituto dentro do sandbox, por padrão: ele executa o comando já confinado pelo sandmux, sem montar nada, e aplica o que o pedido traz e dá para aplicar sem montagem.

## What Changes

- **O `bwrap` do sandmux:** o `sandmux-run`, chamado como `bwrap`, faz o papel do bubblewrap. Ele fica montado em `/run/sandmux/bin/bwrap`, que vem antes no `PATH`, e no lugar do `bwrap` do sistema quando ele existe na visão. Ele:
  - entende as opções do bubblewrap 0.11. As opções desconhecidas encerram com erro, como no bubblewrap;
  - aplica um Landlock a mais, empilhado só no comando, pelas montagens pedidas: `--ro-bind` e `--ro-bind-try` dão leitura, `--bind`, `--bind-try` e `--dev-bind` dão leitura e escrita, e `--tmpfs` dá escrita onde o destino já existe. O que não foi pedido fica fora. O que o pedido cobre com `/dev/null` (o padrão de negar um arquivo) fica sem acesso, quando o Landlock consegue expressar isso;
  - cria os namespaces que não precisam de montagem (`--unshare-user`, `--unshare-net`, `--unshare-ipc`, `--unshare-uts`, `--unshare-pid` com o processo 1 próprio), e aplica `--hostname`, `--chdir`, `--setenv`, `--unsetenv`, `--clearenv`, `--die-with-parent`, `--new-session`, `--cap-drop` e o filtro de `--seccomp`, empilhado sobre o do sandmux;
  - responde ao `--info-fd`, ao `--json-status-fd`, ao `--sync-fd` e ao `--block-fd` como o bubblewrap, para quem os espera não travar;
  - ignora as montagens em si (`--proc`, `--dev`, `--dir`, `--symlink`, `--remount-ro`, `--chmod`, os `--*-fd` de montagem), porque o comando já vê a visão da aba;
  - registra a chamada no registro de execuções, como qualquer execução.
- **Presets dos harnesses:** a documentação diz como cada um isola os comandos dentro do sandmux.
- **Roadmap:** o modo de montagens e o podman ficam em "Depois da v1", com o motivo.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `sandbox-isolation`: o requisito de user namespaces aninhados passa a cobrir o `bwrap` do sandmux.

## Impact

- `crates/sandmux-launcher` (um módulo `bwrap`) e `crates/sandmux/src/bin/sandmux-run.rs`, pelo nome do executável.
- `crates/sandmux-policy/src/compose.rs`: a montagem do `bwrap` na visão.
- Testes: unitários da leitura das opções e integração com um bubblewrap de verdade como referência; o Claude Code real com o sandbox do bash ligado.
- `docs/` e os pares em inglês.
