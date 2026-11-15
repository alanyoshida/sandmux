## MODIFIED Requirements

### Requirement: Contexto enviado ao modelo
Enter com uma pergunta MUST enviá-la ao modelo pelo daemon, numa execução confinada como a da aba de chat, sem criar aba. MUST ir ao modelo só a pergunta, a pasta atual do processo em primeiro plano da aba ativa, o shell em uso e um texto de sistema que pede um único comando, sem explicação. Quando a pasta atual não pode ser lida, MUST ir a pasta de trabalho da aba. O modal MUST mostrar a pasta que vai ao modelo. A tela da aba MUST ir ao modelo só quando o usuário liga o campo "tela" do modal naquela pergunta: então vão as últimas linhas visíveis da aba, no máximo 40, sem as linhas vazias do fim. O campo MUST começar desligado a cada abertura do modal. A TUI MUST continuar respondendo enquanto espera, com o modal mostrando a espera.

#### Scenario: O que o provedor recebe
- **WHEN** a aba ativa é um shell bash em `/home/ana/app` e o usuário pergunta "listar arquivos modificados hoje"
- **THEN** o provedor recebe a pergunta, `/home/ana/app` e `bash`, e nenhuma linha da tela da aba

#### Scenario: Depois de um cd
- **WHEN** o usuário deu `cd src/api` no shell da aba aberta em `/home/ana/app` e abre o modal de comando
- **THEN** o modal mostra `/home/ana/app/src/api`, e o provedor recebe essa pasta

#### Scenario: Tela pedida
- **WHEN** a aba mostra `erro: arquivo travado` e o usuário liga o campo "tela" antes de perguntar
- **THEN** o provedor recebe a pergunta e as últimas linhas da tela, inclusive `erro: arquivo travado`
