# sandbox-isolation Specification

## Purpose

Define como um processo é confinado ao ser lançado pelo sandmux: o que ele deixa de alcançar no host, o que acontece quando a máquina não tem o suporte necessário, que ambiente ele recebe e como o seu ciclo de vida é conduzido.

## Requirements

### Requirement: Lançamento por especificação resolvida
O lançador `sandmux-run` MUST receber uma especificação de lançamento completa e versionada e aplicar somente o que ela descreve. O lançador MUST recusar uma especificação de versão desconhecida, malformada ou com campo que ele não entende, sem executar nenhum comando.

#### Scenario: Especificação válida
- **WHEN** o lançador recebe uma especificação válida da versão suportada
- **THEN** ele monta o confinamento descrito e executa o comando indicado dentro dele

#### Scenario: Versão desconhecida
- **WHEN** o lançador recebe uma especificação com versão que ele não suporta
- **THEN** ele termina com erro, informa a versão recebida e a suportada, e não executa o comando

#### Scenario: Campo desconhecido
- **WHEN** a especificação contém um campo que o lançador não conhece
- **THEN** ele termina com erro e não executa o comando

### Requirement: Falha fechada sem suporte do kernel
O lançador MUST verificar, antes de executar o comando, que a máquina permite user namespaces sem privilégio e oferece Landlock com escopo de sockets abstratos e de sinais. Faltando qualquer um, o lançador MUST recusar o lançamento. Não existe opção para lançar sem confinamento.

#### Scenario: Landlock ausente ou antigo
- **WHEN** o kernel não oferece Landlock com escopo de sockets abstratos
- **THEN** o lançamento é recusado, a mensagem diz qual suporte falta e nenhum comando é executado

#### Scenario: User namespace bloqueado
- **WHEN** a máquina não permite criar user namespace sem privilégio
- **THEN** o lançamento é recusado e a mensagem aponta para `sandmux doctor`

#### Scenario: Falha ao montar o confinamento
- **WHEN** qualquer etapa da montagem do confinamento falha
- **THEN** o comando não é executado e o lançador termina com erro

### Requirement: Processos do host fora do alcance
Um processo confinado MUST enxergar apenas os processos do próprio sandbox e MUST NOT conseguir sinalizar, inspecionar ou depurar um processo do host.

#### Scenario: Listagem de processos
- **WHEN** um processo confinado lista os processos pelo `/proc`
- **THEN** só aparecem processos do próprio sandbox

#### Scenario: Sinal para o host
- **WHEN** um processo confinado tenta enviar um sinal a um processo do host
- **THEN** a tentativa falha

#### Scenario: Depuração dentro do sandbox
- **WHEN** um depurador confinado se anexa a outro processo do mesmo sandbox
- **THEN** a operação funciona

### Requirement: Sockets do host fora do alcance
Um processo confinado MUST NOT conseguir se conectar a sockets Unix do host, nem por caminho nem por nome abstrato. Isso inclui ssh-agent, D-Bus, Docker, Wayland e X11.

#### Scenario: Socket por caminho
- **WHEN** um processo confinado tenta abrir o socket do Docker, do ssh-agent ou do D-Bus do host pelo caminho
- **THEN** o caminho não existe dentro do sandbox

#### Scenario: Socket abstrato
- **WHEN** um processo confinado tenta se conectar a um socket abstrato criado por um processo do host
- **THEN** a conexão é negada

#### Scenario: Socket próprio
- **WHEN** dois processos do mesmo sandbox se comunicam por um socket Unix criado lá dentro
- **THEN** a comunicação funciona

### Requirement: Sem privilégio dentro do sandbox
O comando confinado MUST rodar com a mesma identidade numérica do usuário, sem capabilities e sem possibilidade de ganhar privilégio. Programas setuid MUST NOT elevar privilégio dentro do sandbox.

#### Scenario: Identidade
- **WHEN** um processo confinado consulta a própria identidade
- **THEN** ele vê o UID e o GID do usuário que lançou o sandbox, e não o de root

#### Scenario: Tentativa de elevação
- **WHEN** um processo confinado executa `sudo` ou outro programa setuid
- **THEN** o programa não ganha privilégio

### Requirement: Chamadas de sistema perigosas negadas com erro
O sandbox MUST negar ao processo confinado as chamadas de montagem e troca de raiz, `bpf`, `perf_event_open`, carga de módulos, `kexec`, o chaveiro do kernel, `userfaultfd` e `io_uring`. A chamada negada MUST devolver erro de permissão, sem encerrar o processo.

#### Scenario: Chamada negada
- **WHEN** um processo confinado usa uma das chamadas negadas
- **THEN** a chamada devolve erro de permissão e o processo continua rodando

#### Scenario: Chaveiro do kernel
- **WHEN** um processo confinado tenta ler chaves do chaveiro de sessão do usuário
- **THEN** a chamada devolve erro de permissão

### Requirement: Injeção de teclas no terminal negada
Um processo confinado MUST NOT conseguir inserir caracteres na fila de entrada do terminal do usuário.

#### Scenario: Injeção de teclas
- **WHEN** um processo confinado usa o controle de terminal que insere caracteres na entrada
- **THEN** a chamada devolve erro e nada é inserido no terminal

### Requirement: User namespaces aninhados permitidos, sem montagem
Um processo confinado MUST poder criar user namespaces próprios, para que programas que se isolam continuem funcionando. Dentro de um namespace aninhado, o processo MUST continuar sem conseguir montar nada nem alcançar o que o sandbox esconde.

#### Scenario: Navegador headless
- **WHEN** um navegador headless que cria o próprio isolamento é iniciado dentro do sandbox
- **THEN** ele inicia

#### Scenario: Montagem em namespace aninhado
- **WHEN** um processo confinado cria um user namespace e tenta montar um sistema de arquivos
- **THEN** a montagem falha

### Requirement: Ambiente construído do zero
O comando confinado MUST receber apenas as variáveis de ambiente de uma lista segura, as que o preset define e as passadas com `--pass-env`. A lista segura é: `TERM`, `COLORTERM`, `TERM_PROGRAM`, `TERM_PROGRAM_VERSION`, `LANG`, `LC_*`, `TZ`, `NO_COLOR`, `USER`, `LOGNAME`, `SHELL`, `HOME`, `PATH` reconstruído e as variáveis XDG apontando para o HOME sintético.

#### Scenario: Variável com segredo no terminal do usuário
- **WHEN** o usuário tem uma variável de API key exportada no terminal e roda `sandmux run` sem `--pass-env`
- **THEN** a variável não existe dentro do sandbox

#### Scenario: Variáveis de sessão do desktop
- **WHEN** o terminal do usuário tem `SSH_AUTH_SOCK`, `DBUS_SESSION_BUS_ADDRESS`, `DISPLAY` e `WAYLAND_DISPLAY` definidas
- **THEN** nenhuma delas existe dentro do sandbox

#### Scenario: Ambiente do processo pai
- **WHEN** um processo confinado tenta ler o ambiente do processo que o lançou
- **THEN** ele não tem acesso a esse processo

### Requirement: Passagem explícita de variável de ambiente
`sandmux run` MUST aceitar `--pass-env <NOME>`, repetível, que copia a variável indicada do ambiente do usuário para dentro do sandbox. A opção é desligada por padrão. A cada execução com `--pass-env`, o `run` MUST avisar, antes de lançar, quais variáveis ficam ao alcance do agente, sem mostrar os valores.

#### Scenario: Variável passada
- **WHEN** o usuário roda `sandmux run opencode --pass-env OPENROUTER_API_KEY`
- **THEN** a variável existe dentro do sandbox com o valor do ambiente do usuário, e o aviso lista o nome dela

#### Scenario: Variável inexistente
- **WHEN** a variável indicada em `--pass-env` não existe no ambiente do usuário
- **THEN** o `run` termina com erro e não lança o sandbox

### Requirement: Terminal do usuário entregue ao comando
O comando confinado MUST rodar em primeiro plano no terminal de onde o `run` foi chamado, com entrada, saída e redimensionamento funcionando como numa execução direta.

#### Scenario: Interface de terminal do harness
- **WHEN** um harness com interface de tela cheia é lançado
- **THEN** ele desenha a interface, recebe teclado e mouse, e acompanha o redimensionamento da janela

### Requirement: Ciclo de vida e código de saída
O `run` MUST terminar com o mesmo código de saída do comando confinado e MUST repassar a ele os sinais de término recebidos. Quando o comando confinado termina, todos os processos restantes do sandbox MUST ser encerrados. Processos órfãos dentro do sandbox MUST ser recolhidos.

#### Scenario: Código de saída
- **WHEN** o comando confinado termina com código 3
- **THEN** `sandmux run` termina com código 3

#### Scenario: Processos em segundo plano
- **WHEN** o harness termina deixando processos em segundo plano no sandbox
- **THEN** esses processos são encerrados e nenhum continua rodando no host

#### Scenario: Sinal de término
- **WHEN** o `run` recebe um sinal de término
- **THEN** o comando confinado recebe o sinal e o sandbox é desfeito

#### Scenario: Processo órfão
- **WHEN** um processo do sandbox termina depois que o pai dele já saiu
- **THEN** ele é recolhido e não permanece como zumbi

### Requirement: Rede do host sem filtro nesta fatia
Nesta fatia, o processo confinado MUST usar a rede do host, sem filtro e sem registro. O `sandmux doctor` MUST informar que o controle de rede ainda não está ativo.

#### Scenario: Acesso à internet
- **WHEN** um processo confinado abre uma conexão para um servidor externo
- **THEN** a conexão funciona como no host

#### Scenario: Diagnóstico do nível de proteção
- **WHEN** o usuário roda `sandmux doctor`
- **THEN** a saída diz que a rede não é filtrada nem registrada

### Requirement: Diagnóstico de suporte da máquina
`sandmux doctor` MUST mostrar o resultado de cada verificação de suporte (user namespaces sem privilégio, versão do Landlock, camada de escrita com overlay) e, para cada falha, a causa e o que fazer. O comando MUST terminar com código diferente de zero quando o sandbox não pode ser lançado.

#### Scenario: Máquina com suporte completo
- **WHEN** todas as verificações passam
- **THEN** a saída lista cada verificação como atendida e o comando termina com código zero

#### Scenario: Restrição de user namespace por AppArmor
- **WHEN** a distro restringe user namespaces sem privilégio por AppArmor
- **THEN** a saída explica a causa, mostra o perfil AppArmor necessário para o lançador e o comando que o instala, e termina com código diferente de zero

#### Scenario: Camada de escrita indisponível
- **WHEN** o overlay em user namespace não funciona na máquina
- **THEN** a saída avisa que caches e toolchains vão funcionar no modo de recuo, e o comando termina com código zero
