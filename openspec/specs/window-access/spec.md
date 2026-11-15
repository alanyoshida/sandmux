# window-access Specification

## Purpose
Define como o sandbox recebe, só quando o usuário liga, as janelas e o áudio do desktop: o modo restrito pelo security-context do compositor com o repassador do Wayland que esconde os protocolos privilegiados, o modo direto, o áudio, o ambiente gráfico, o aviso e a marca na TUI.

## Requirements

### Requirement: Opção de janela
O `sandmux run` MUST aceitar `--window`, `--no-window` e `--window-direct`, e a configuração MUST aceitar `window = true`, `false` ou `"direct"` no global, nos perfis e nos projetos, com a mesma precedência das outras chaves de liga e desliga. O padrão MUST ser desligado.

#### Scenario: Padrão
- **WHEN** o usuário roda `sandmux run opencode` sem a opção e sem a chave
- **THEN** nenhum socket do compositor ou do áudio entra no sandbox

#### Scenario: Opção vence a configuração
- **WHEN** a configuração tem `window = true` e o usuário passa `--no-window`
- **THEN** a aba é lançada sem janela

### Requirement: Wayland pelo security-context
Com `window = true`, o sandmux MUST registrar no compositor, pelo `wp_security_context_v1`, um socket de escuta da aba com o motor `sandmux`, o app_id `sandmux` e o identificador da aba. O sandbox MUST receber só o socket do repassador da aba, ligado a ele. Os dois sockets MUST parar de aceitar conexões quando a aba termina. Sem o protocolo, ou sem sessão Wayland, o lançamento MUST ser recusado com a explicação e a sugestão de `window = "direct"`.

#### Scenario: Compositor com o protocolo
- **WHEN** a aba é lançada com `window = true` num compositor que anuncia `wp_security_context_manager_v1`
- **THEN** um programa gráfico do sandbox abre a janela no desktop, conectado pelo socket da aba

#### Scenario: Compositor sem o protocolo
- **WHEN** a aba é lançada com `window = true` num compositor sem o protocolo
- **THEN** o lançamento é recusado, explicando o motivo e o modo direto

#### Scenario: Aba encerrada
- **WHEN** a aba termina
- **THEN** os sockets da aba não aceitam conexões novas e são apagados

### Requirement: Protocolos privilegiados escondidos
No modo restrito, o repassador MUST mostrar ao sandbox só os globais do Wayland de uma allowlist de protocolos comuns (janela, buffers, entrada com foco, clipboard por foco, escala e aparência); os demais, como o clipboard em segundo plano, a captura de tela e o teclado virtual, MUST ficar fora do registro. Um `bind` num global escondido MUST derrubar a conexão. As outras mensagens e os descritores MUST passar sem alteração.

#### Scenario: Clipboard em segundo plano
- **WHEN** o compositor anuncia `ext_data_control_manager_v1` e um programa do sandbox lista os globais
- **THEN** o global não aparece

#### Scenario: Bind escondido
- **WHEN** um programa do sandbox faz `bind` num global que o repassador escondeu
- **THEN** a conexão dele com o compositor é encerrada

### Requirement: Janela só nas abas do daemon
A janela MUST funcionar só nas abas do daemon, que mantém o repassador e o socket da aba. Sem terminal, o `run` com a janela ligada MUST terminar com erro, explicando que a janela precisa de uma aba do daemon.

#### Scenario: Sem terminal
- **WHEN** um script roda `sandmux run shell --window -- -c comando`
- **THEN** o comando termina com erro, sem lançar o sandbox

### Requirement: Wayland direto
Com `window = "direct"`, o sandmux MUST montar no sandbox o socket comum do compositor do usuário.

#### Scenario: Modo direto
- **WHEN** a aba é lançada com `window = "direct"`
- **THEN** o socket do compositor do usuário está no sandbox, e o aviso do lançamento diz que o agente pode ler o clipboard e talvez capturar a tela

### Requirement: Áudio com a janela
Com a janela ligada, em qualquer modo, os sockets `pipewire-0` e `pulse/native` do usuário MUST entrar no sandbox, quando existem.

#### Scenario: Som
- **WHEN** a aba tem a janela ligada e o host tem o PipeWire
- **THEN** um programa do sandbox toca som pelo PipeWire ou pelo PulseAudio

### Requirement: Nunca X11
Nenhum socket do X11 MUST entrar no sandbox, com ou sem a janela, e `DISPLAY` MUST NOT ser definida.

#### Scenario: App só X11
- **WHEN** um programa que só fala X11 é aberto numa aba com janela
- **THEN** ele não abre, por falta de display

### Requirement: Ambiente gráfico no sandbox
Com a janela ligada, o sandbox MUST ter `XDG_RUNTIME_DIR` numa pasta privada em memória com os sockets montados nela, `WAYLAND_DISPLAY`, e as variáveis que fazem os toolkits usarem o Wayland: `GDK_BACKEND=wayland`, `QT_QPA_PLATFORM=wayland`, `MOZ_ENABLE_WAYLAND=1`, `ELECTRON_OZONE_PLATFORM_HINT=wayland` e `SDL_VIDEODRIVER=wayland`.

#### Scenario: Variáveis
- **WHEN** a aba tem a janela ligada
- **THEN** `WAYLAND_DISPLAY` e `XDG_RUNTIME_DIR` estão definidas e o socket existe em `$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY`

### Requirement: Aviso no lançamento
Antes de lançar uma aba com a janela ligada, o `sandmux run` MUST avisar que o agente pode abrir janelas no desktop, tocar som, gravar o microfone e ouvir os outros apps. No modo direto, o aviso MUST dizer também que o agente pode ler o clipboard e, dependendo do compositor, capturar a tela.

#### Scenario: Aviso restrito
- **WHEN** o usuário roda `sandmux run shell --window`
- **THEN** o aviso aparece antes do shell, sem a frase do clipboard

### Requirement: Marca na barra da TUI
A barra da TUI MUST marcar com `▣` as abas lançadas com a janela ligada.

#### Scenario: Aba com janela
- **WHEN** uma aba foi lançada com `window = true`
- **THEN** a barra mostra `▣` no nome dela, e as outras abas não
