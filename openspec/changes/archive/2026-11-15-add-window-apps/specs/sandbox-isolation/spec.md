## MODIFIED Requirements

### Requirement: Sockets do host fora do alcance
Um processo confinado MUST NOT conseguir se conectar a sockets Unix do host, nem por caminho nem por nome abstrato. Isso inclui ssh-agent, D-Bus, Docker, Wayland e X11. A exceção são os sockets do compositor e do áudio que a opção `window` monta.

#### Scenario: Socket por caminho
- **WHEN** um processo confinado tenta abrir o socket do Docker, do ssh-agent ou do D-Bus do host pelo caminho
- **THEN** o caminho não existe dentro do sandbox

#### Scenario: Socket abstrato
- **WHEN** um processo confinado tenta se conectar a um socket abstrato criado por um processo do host
- **THEN** a conexão é negada

#### Scenario: Socket próprio
- **WHEN** dois processos do mesmo sandbox se comunicam por um socket Unix criado lá dentro
- **THEN** a comunicação funciona

#### Scenario: Janela ligada
- **WHEN** a aba tem a opção `window`
- **THEN** só o socket do compositor da aba e os do áudio entram; o D-Bus e o X11 continuam fora

### Requirement: Ambiente construído do zero
O comando confinado MUST receber apenas as variáveis de ambiente de uma lista segura, as que o preset define e as passadas com `--pass-env`. A lista segura é: `TERM`, `COLORTERM`, `TERM_PROGRAM`, `TERM_PROGRAM_VERSION`, `LANG`, `LC_*`, `TZ`, `NO_COLOR`, `USER`, `LOGNAME`, `SHELL`, `HOME`, `PATH` reconstruído e as variáveis XDG apontando para o HOME sintético. Com a opção `window`, entram também as variáveis gráficas que ela define.

#### Scenario: Variável com segredo no terminal do usuário
- **WHEN** o usuário tem uma variável de API key exportada no terminal e roda `sandmux run` sem `--pass-env`
- **THEN** a variável não existe dentro do sandbox

#### Scenario: Variáveis de sessão do desktop
- **WHEN** o terminal do usuário tem `SSH_AUTH_SOCK`, `DBUS_SESSION_BUS_ADDRESS`, `DISPLAY` e `WAYLAND_DISPLAY` definidas e a aba não tem a opção `window`
- **THEN** nenhuma delas existe dentro do sandbox

#### Scenario: Ambiente do processo pai
- **WHEN** um processo confinado tenta ler o ambiente do processo que o lançou
- **THEN** ele não tem acesso a esse processo
