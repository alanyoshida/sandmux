## Context

O sandbox é montado numa raiz nova (`LaunchSpec.mounts`), e o ambiente é construído do zero. Sockets já entram assim nas sessões do daemon: o Docker privado (`/run/sandmux/docker.sock`) e o agente SSH (`/run/sandmux/ssh-agent.sock`), criados pelo daemon por sessão e guardados na sessão enquanto ela vive. A GPU (render nodes) já entra por padrão.

Nesta máquina, o desktop é o KDE Plasma 6.6 em Wayland (`wayland-0`), com o PipeWire (`pipewire-0`, `pulse/native`). O KWin anuncia `wp_security_context_manager_v1` e `ext_data_control_manager_v1`, este último o protocolo que deixa qualquer cliente comum ler o clipboard em segundo plano. Os crates `wayland-client` e `wayland-protocols` já estão no `Cargo.lock`.

## Goals / Non-Goals

**Goals:**
- Um agente abre janelas (navegador, app que ele construiu) e toca som, só quando o usuário liga.
- Por padrão, o compositor sabe que o cliente é de um sandbox e aplica a política dele.
- Sempre visível quais abas têm acesso ao desktop.

**Non-Goals:**
- X11 e XWayland, em qualquer modo.
- Portal do desktop e D-Bus: change seguinte.
- Separar tocar de gravar no áudio: exigiria mudar a configuração do PipeWire do usuário.
- Escolher quais protocolos o compositor nega: é a política do compositor.

## Decisions

- **Valor de três estados:** `window` aceita `true`, `false` e `"direct"`, com a precedência das chaves de liga e desliga (a camada mais específica vence, e as opções do `run` vencem a configuração). `--window-direct` liga o modo direto. Na TUI, o item da tela de configurações alterna entre os três.
- **Security-context no lado do host:** um módulo `window` no crate `sandmux` conecta ao compositor pelo caminho `$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY` do cliente que pediu a aba (as variáveis vêm com o pedido, como as do `--pass-env`), procura `wp_security_context_manager_v1` no registro, cria um `UnixListener` numa pasta privada do sandmux em `$XDG_RUNTIME_DIR/sandmux/window/<id>.sock` e um pipe de fechamento, e chama `create_listener`, `set_sandbox_engine("sandmux")`, `set_app_id("sandmux")`, `set_instance_id(<id>)` e `commit`, com um roundtrip para confirmar. O resultado é um `WindowSocket` com o caminho e a ponta de escrita do pipe. Soltá-lo fecha o pipe (o compositor para de aceitar conexões novas) e apaga o arquivo.
- **Repassador (`wlproxy`):** o KWin 6.6 desta máquina esconde dos clientes com security-context só o próprio `wp_security_context_manager_v1`; o `ext_data_control_manager_v1` continua visível, e com ele a leitura do clipboard em segundo plano. Decisão do usuário (2026-11-15): um repassador no sandmux. O daemon escuta em `<id>.sock` (montado no sandbox) e liga cada conexão ao socket do security-context (`<id>.ctx`). Ele lê só o registro: guarda os ids criados por `wl_display.get_registry`, tira os `wl_registry.global` cujas interfaces não estão na allowlist (e o `global_remove` deles) e derruba a conexão num `bind` de nome escondido ou de interface fora da lista. As mensagens são repassadas inteiras; os descritores (SCM_RIGHTS) vão junto com os primeiros bytes repassados depois deles, o que o `libwayland` aceita, porque enfileira os descritores até a mensagem que os usa. Duas threads por conexão.
- **Allowlist, não denylist:** um protocolo novo de um compositor fica escondido até entrar na lista. A lista tem os protocolos de janela, buffer (shm, dmabuf, syncobj), entrada com foco (seat, text-input, ponteiro), clipboard por foco (`wl_data_device_manager`, primary selection), escala, cor, decoração e os efeitos do KWin. Verificado no KWin: o `kdialog` abre pela janela restrita, e ficam fora o data-control, o layer-shell, o plasma-shell, a captura de atalhos e o gerenciamento das telas.
- **Quem segura:** o `WindowSocket` (repassador e socket do security-context) fica na sessão viva do daemon, como o agente SSH, e é solto quando o sandbox termina. O `run` sem terminal troca de programa (`exec`) e não teria quem mantivesse o repassador: a janela é recusada nele.
- **Modo direto:** a montagem do `$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY` do cliente, sem conexão ao compositor.
- **Dentro do sandbox:** `Tmpfs` em `/run/user/<uid>` (modo 700) como `XDG_RUNTIME_DIR`; o socket da aba (ou o direto) montado em `/run/user/<uid>/wayland-0`; `pipewire-0` e `pulse/native` montados ao lado, quando existem no host. `WAYLAND_DISPLAY=wayland-0` e as dicas dos toolkits. Nada de `DISPLAY`.
- **Erros:** sem `WAYLAND_DISPLAY` no cliente, sem o socket, sem o protocolo ou com a conexão recusada, o lançamento falha com a explicação. No modo restrito, a mensagem sugere `window = "direct"` e diz o que ele libera.
- **Restauração:** a lista de sessões guarda o modo e o `WAYLAND_DISPLAY`. Ao relançar (reinício do daemon, primeiro login), o daemon refaz o socket; se o compositor não estiver pronto, a aba volta encerrada com a mensagem, e Ctrl+b `r` tenta de novo.
- **Protocolo 34:** `window: Option<WindowMode>` no item da lista de sessões, para a marca `▣` na barra.
- **Aviso:** no `run` com terminal e sem terminal, no mesmo formato do aviso do `--pass-env`. Na TUI, a marca na barra faz esse papel.
- **Teste sem desktop:** um compositor falso no teste fala o protocolo do Wayland pelo socket (registro, sync, bind e o `create_listener` com os descritores por `SCM_RIGHTS`), com ou sem o `wp_security_context_manager_v1` anunciado.

## Risks / Trade-offs

- [Um app que precisa de um protocolo fora da allowlist] → Ele perde só aquele recurso (em geral opcional) ou não abre; a lista cresce quando aparecer o caso. O modo direto continua como recurso explícito.
- [Latência do repassador] → Uma cópia a mais por mensagem, no daemon; os buffers grandes (shm, dmabuf) não passam por ele, só os descritores.
- [Áudio expõe o microfone] → O aviso diz isso a cada lançamento, e o áudio só vem com a janela.
- [Um app gráfico lento ou travado no sandbox] → Fica nos tetos de memória e processos da aba, como qualquer outro.
- [Compositor que ainda não subiu na restauração] → A aba volta encerrada com a mensagem, sem relançar sem janela.
