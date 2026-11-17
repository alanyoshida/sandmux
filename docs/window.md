# Janelas e áudio do desktop

> English: [en/window.md](en/window.md)

Por padrão, nada do desktop entra no sandbox: nenhum socket do Wayland, do X11, do PipeWire ou do seu D-Bus, e as variáveis da sessão gráfica são apagadas. A opção `window` deixa o agente abrir janelas no seu desktop (um navegador para testar uma página, o app que ele construiu) e usar o áudio. O [portal do desktop](#portal-do-desktop), ligado por padrão, dá o tema, as notificações e o abrir link por um D-Bus próprio do sandmux.

## Ligar

```sh
sandmux run opencode --window          # restrito, pelo security-context
sandmux run opencode --window-direct   # direto, pelo socket comum
sandmux run opencode --no-window       # desliga, mesmo com a configuração
sandmux config set --global window true
```

Na configuração, `window` aceita `true`, `false` ou `"direct"`, no global, nos perfis e nos projetos, com a mesma precedência das outras chaves de liga e desliga ([config.md](config.md)). Na TUI, o item "Janela" da seção Sandbox, em Ctrl+b `e`, alterna entre os três. O padrão é desligado.

- **Só nas abas do daemon:** a janela funciona nas abas abertas num terminal ou pela TUI. Sem terminal (script, pipe), o `run` com a janela termina com erro, porque o repassador do Wayland vive no daemon.
- **Aviso:** antes de lançar, o `run` avisa que o agente pode abrir janelas, tocar som, gravar o microfone e ouvir os outros apps. No modo direto, o aviso diz também que ele pode ler o clipboard e, dependendo do compositor, capturar a tela.
- **Marca:** a barra da TUI mostra `▣` no nome das abas com a janela ligada.

## Modo restrito (`true`)

1. O sandmux registra no compositor um socket próprio da aba, pelo protocolo `wp_security_context_v1`, com o motor e o app_id `sandmux` e o identificador da aba. Assim o compositor sabe que os clientes são de um sandbox.
2. Na frente dele, o daemon põe um repassador do protocolo do Wayland, e é o socket do repassador que entra no sandbox.
3. O repassador mostra ao sandbox só os globais de uma allowlist de protocolos comuns: janela, buffers (shm, dmabuf), entrada com foco (teclado, ponteiro, text-input), clipboard por foco (o copiar e colar normal), escala, cor, decoração e os efeitos do KWin.
4. Fica escondido o resto, como:
   - o clipboard em segundo plano (`ext_data_control`, `zwlr_data_control`);
   - a captura de tela e o teclado e o mouse virtuais;
   - os métodos de entrada;
   - a lista e o controle das janelas dos outros programas;
   - as camadas por cima do desktop (`zwlr_layer_shell`, `org_kde_plasma_shell`);
   - a trava de tela e a captura de atalhos;
   - qualquer protocolo que não está na lista.
5. Um programa que tenta usar um global escondido tem a conexão encerrada.

O repassador existe porque o compositor sozinho pode não bastar: o KWin 6.6 esconde dos clientes com security-context só o próprio protocolo de security-context, e o clipboard em segundo plano continuaria ao alcance do agente.

Sem o `wp_security_context_v1` (KDE 6 e sway têm; outros compositores podem não ter), ou sem sessão Wayland, o lançamento é recusado com a explicação. O modo direto é o recurso explícito.

## Modo direto (`"direct"`)

O socket comum do seu compositor entra no sandbox, sem o repassador. O agente vê todos os protocolos que o compositor oferece a um app comum: no KDE, inclusive o clipboard em segundo plano.

## Áudio

Com a janela ligada, em qualquer modo, os sockets `pipewire-0` e `pulse/native` do seu usuário também entram, quando existem. O PipeWire não separa tocar de gravar: o agente pode tocar som, gravar o microfone e ouvir os outros apps. Separar exigiria mudar a configuração do PipeWire do sistema, o que o sandmux não faz.

## Dentro do sandbox

- `XDG_RUNTIME_DIR=/run/user/<uid>`, uma pasta privada em memória, com `wayland-0`, `pipewire-0` e `pulse/native`;
- `WAYLAND_DISPLAY=wayland-0` e `XDG_SESSION_TYPE=wayland`;
- dicas para os toolkits usarem o Wayland: `GDK_BACKEND`, `QT_QPA_PLATFORM`, `MOZ_ENABLE_WAYLAND`, `ELECTRON_OZONE_PLATFORM_HINT`, `SDL_VIDEODRIVER`.

O X11 nunca entra: `DISPLAY` não existe, e nenhum socket do X11 é montado, porque o X11 permite capturar o teclado do desktop inteiro. Um programa que só fala X11 não abre. A GPU já entra por padrão ([config.md](config.md)).

## Navegadores

O Chrome, o Chromium e os apps em Electron sobem com o sandbox deles (namespaces e seccomp), sem `--no-sandbox`: o sandmux deixa um processo do sandbox criar user e pid namespaces e mapear neles o próprio uid, como fora do sandbox. O perfil fica no HOME sintético do harness, e a rede passa pelo gateway, com a política da aba.

```sh
google-chrome https://localhost:3000
```

No modo `--headless`, o navegador funciona sem a janela.

## Portal do desktop

Toda aba do daemon ganha, por padrão, um D-Bus próprio do sandmux, que não é o seu D-Bus de sessão. Por ele, um programa confinado lê o tema, manda notificações e abre links no seu navegador. Qualquer outro serviço do D-Bus recebe erro, e nenhuma mensagem do sandbox chega ao seu D-Bus.

```sh
sandmux run opencode --no-portal        # sem o portal
sandmux config set --global portal false
```

A chave `portal` aceita `true` ou `false`, no global, nos perfis e nos projetos ([config.md](config.md)). Na TUI, o item "Portal" fica na seção Sandbox das configurações, e o painel de proteção mostra o estado. Sem terminal (script, pipe), o `run` segue sem portal, e um `--portal` explícito termina com erro.

- **Aparência:** o `org.freedesktop.portal.Settings` responde só `org.freedesktop.appearance` (`color-scheme`, `accent-color` e `contrast`), com os valores do portal do seu desktop. Sem portal no desktop, a resposta é "sem preferência".
- **Notificações:** o portal (`org.freedesktop.portal.Notification`) e o `org.freedesktop.Notifications` direto, que o `notify-send` usa.
  - As notificações chegam ao seu desktop com o nome da aba no título, como `shell:app: Pronto`.
  - Título e corpo são cortados, e a marcação vira texto.
  - Ícones, ações, sons e imagens do sandbox não passam.
  - O limite é de 5 notificações a cada 10 segundos por aba; o excesso é descartado sem erro.
- **Abrir link:** o `OpenURI` do portal abre um link `http` ou `https` direto no seu navegador padrão, mas só quando a política de rede da aba permite o domínio ([network.md](network.md)).
  - No modo só inferência, por exemplo, só os provedores de modelo abrem.
  - Cada pedido, aberto ou recusado, entra no registro de rede da aba (`sandmux net`), com o protocolo `open-uri`.
  - Arquivos e pastas (`OpenFile`, `OpenDirectory`) são recusados.

### Como os programas chegam ao portal

- **Variáveis:** `DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/<uid>/bus` e `GIO_USE_PORTALS=1`, numa pasta `XDG_RUNTIME_DIR` em memória. Com a janela ligada, é a mesma pasta.
- **`xdg-open`:** um `flatpak-info` vazio nessa pasta faz o `xdg-open` (e o pacote `open` do Node, que traz uma cópia dele) abrir links pelo portal.
- **O socket:** o `bus` é criado dentro do sandbox por um repassador do processo 1, sem capabilities e com o seccomp. Ele liga cada conexão ao barramento, que vive no daemon. Um socket montado de fora seria negado pelo AppArmor de programas com perfil, como o `notify-send` do Ubuntu.

## Limites

- **Só os três serviços do portal:** um app que pede um diálogo do sistema (o seletor de arquivos), a captura de tela, o keyring ou outro serviço do D-Bus recebe erro.
- **Apps da GIO:** um app da GIO prefere o programa que trata `https`, e só usa o portal quando não acha nenhum. Sem a janela, o navegador desse programa não abre. O `xdg-open` e os programas que o chamam usam o portal.
- Um app que precisa de um protocolo fora da allowlist perde aquele recurso, em geral opcional, ou não abre.
- Depois de um reinício do daemon, a aba refaz o socket. Se o compositor ainda não subiu, ela volta encerrada com a mensagem, e Ctrl+b `r` tenta de novo.
