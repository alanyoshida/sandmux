# shell-command-assistant Specification

## Purpose
Define o modal de comando: a pergunta ao modelo com só a pasta da aba e o shell, a limpeza da resposta e a escrita do comando na linha da aba sem executar.

## Requirements

### Requirement: Modal de comando
Na TUI, a tecla de prefixo seguida de `g` MUST abrir o modal de comando sobre a aba ativa, com um campo de pergunta e um campo de modelo. O modelo MUST começar com `[assistant] command-model` ou, sem ele, com `chat-model`. Tab, as setas no campo de modelo e Esc MUST funcionar como no modal de chat. A ação MUST se chamar `command` em `[tui.keys]`.

#### Scenario: Modal aberto
- **WHEN** `[assistant] command-model` é `openrouter/x` e o usuário digita Ctrl+b g
- **THEN** o modal de comando abre com o modelo `openrouter/x`

### Requirement: Contexto enviado ao modelo
Enter com uma pergunta MUST enviá-la ao modelo pelo daemon, numa execução confinada como a da aba de chat, sem criar aba. MUST ir ao modelo só a pergunta, a pasta de trabalho da aba ativa, o shell em uso e um texto de sistema que pede um único comando, sem explicação. Nada da tela da aba MUST ser enviado. A TUI MUST continuar respondendo enquanto espera, com o modal mostrando a espera.

#### Scenario: O que o provedor recebe
- **WHEN** a aba ativa é um shell bash em `/home/ana/app` e o usuário pergunta "listar arquivos modificados hoje"
- **THEN** o provedor recebe a pergunta, `/home/ana/app` e `bash`, e nenhuma linha da tela da aba

### Requirement: Comando sugerido
A resposta MUST aparecer no modal num campo editável, sem as cercas de código (```` ``` ````) e sem um prompt `$ ` no começo. Um erro do provedor MUST aparecer no modal, que continua aberto.

#### Scenario: Resposta em bloco de código
- **WHEN** o modelo responde "```bash\nls -la\n```"
- **THEN** o campo do comando mostra `ls -la`

### Requirement: Escrita na aba sem executar
Enter no campo do comando, ou o clique no botão "Inserir", MUST escrever o comando na entrada da aba ativa sem quebra de linha no fim e fechar o modal. Um comando de várias linhas MUST ir entre os marcadores de colagem quando a aba ligou o modo de colagem; sem ele, o modal MUST recusar e manter o comando.

#### Scenario: Comando escrito
- **WHEN** o campo do comando tem `ls -la` e o usuário aperta Enter
- **THEN** a linha do shell da aba mostra `ls -la`, e o comando não roda até o usuário apertar Enter na aba

#### Scenario: Várias linhas sem colagem
- **WHEN** o comando tem duas linhas e a aba não ligou o modo de colagem
- **THEN** o modal recusa, explica que a quebra de linha executaria o comando, e o comando continua no campo
