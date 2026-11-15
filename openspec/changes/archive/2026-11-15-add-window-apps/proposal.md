## Why

Item "Programas com janela" da lista depois da v1, escolhido pelo usuário (2026-11-15).

O sandbox não tem acesso ao desktop: nenhum socket do Wayland, do X11, do PipeWire ou do D-Bus entra, e as variáveis de sessão gráfica são apagadas. Um agente não consegue abrir um navegador para testar uma página, nem um app gráfico que ele mesmo construiu. Liberar o socket do compositor sem cuidado, porém, deixa o agente ler o clipboard em segundo plano e, em alguns compositores, capturar a tela.

## What Changes

- **Opção `window`:** `--window`, `--no-window` e `--window-direct` no `sandmux run`, e `window = true | false | "direct"` na configuração (global, perfil, projeto) e na tela de configurações da TUI. Desligada por padrão.
- **Wayland restrito (`window = true`):** o sandmux pede ao compositor um socket próprio da aba pelo protocolo `wp_security_context_v1`, com o motor `sandmux` e o identificador da aba, e põe na frente dele um repassador do protocolo do Wayland, no daemon. O repassador mostra ao sandbox só os globais de uma allowlist de protocolos comuns e esconde os privilegiados (clipboard em segundo plano, captura de tela, teclado e mouse virtuais, métodos de entrada, controle das outras janelas, camadas por cima do desktop). O repassador é necessário porque o KWin 6.6 não esconde o clipboard em segundo plano dos clientes com security-context, conforme verificado nesta máquina. Sem o protocolo, o lançamento é recusado com a explicação.
- **Só nas abas do daemon:** o repassador vive no daemon. Sem terminal, o `run` com a janela é recusado.
- **Wayland direto (`window = "direct"`):** o socket comum do compositor entra, com aviso forte de que o agente pode ler o clipboard e, dependendo do compositor, capturar a tela.
- **Áudio junto:** com a janela, os sockets do PipeWire e do PulseAudio do usuário também entram. O aviso diz que o agente pode tocar som, gravar o microfone e ouvir os outros apps.
- **Nunca X11:** `DISPLAY` continua apagado e nenhum socket do X11 entra. Apps que só falam X11 não abrem.
- **Dentro do sandbox:** `XDG_RUNTIME_DIR` numa pasta privada em memória com `wayland-0`, `pipewire-0` e `pulse/native`, `WAYLAND_DISPLAY`, e as dicas para os toolkits usarem o Wayland (`GDK_BACKEND`, `QT_QPA_PLATFORM`, `MOZ_ENABLE_WAYLAND`, `ELECTRON_OZONE_PLATFORM_HINT`, `SDL_VIDEODRIVER`).
- **Visível:** o `run` avisa antes de lançar, como no `--pass-env`; a barra da TUI marca a aba com `▣`.
- **Protocolo 34:** a lista de sessões traz o modo de janela de cada aba.

O portal do desktop (aparência, notificações e abrir link com a regra de rede) vem no change seguinte.

## Capabilities

### New Capabilities
- `window-access`: janelas e áudio do desktop para o sandbox, pelo security-context do Wayland ou pelo socket direto, com aviso e marca na TUI.

### Modified Capabilities
- `sandbox-isolation`: os sockets do compositor e do áudio e as variáveis gráficas entram só com a opção `window`.

## Impact

- **`sandmux-policy`:** a chave `window` (três valores) nas camadas e no efetivo; montagens e variáveis na composição.
- **`sandmux`:** módulo que cria o socket do security-context (wayland-client e wayland-protocols, já no `Cargo.lock`) e o repassador com a allowlist, mantidos pela sessão do daemon enquanto a aba vive; opções do `run`; aviso; marca na barra; item na tela de configurações.
- **`sandmux-proto`:** o modo de janela na sessão; protocolo 34.
- **Testes:** composição (montagens e variáveis por modo), configuração, um compositor falso que anuncia ou não o protocolo, recusa sem o protocolo, aviso, marca na TUI; verificação manual no KDE.
- **Documentação:** `docs/config.md`, um doc novo `docs/window.md`, README e os pares em inglês.
