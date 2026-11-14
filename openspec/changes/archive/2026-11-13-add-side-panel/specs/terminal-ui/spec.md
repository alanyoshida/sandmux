# Spec Delta

## MODIFIED Requirements

### Requirement: Barra de abas
A TUI MUST mostrar na primeira linha uma aba para cada sessão, com a posição (o número usado pelos atalhos de dígito), o nome e uma marca quando a sessão está encerrada ou parada, destacando a aba ativa. O identificador da sessão aparece na lista de abas. A área abaixo da barra, à esquerda do painel lateral quando ele está visível, MUST ser o terminal da aba ativa, e o tamanho dessa área MUST ser o tamanho do terminal da sessão.

#### Scenario: Aba ativa
- **WHEN** a aba 2 está ativa
- **THEN** a barra destaca a aba 2, e a área do terminal mostra a tela da sessão 2, com uma linha a menos que o terminal do usuário

## ADDED Requirements

### Requirement: Painel lateral
A TUI MUST mostrar à direita um painel de 34 colunas com a seção "Proteção" e a seção "Recursos" da aba ativa, quando o terminal tem pelo menos 100 colunas e o painel não foi escondido. O atalho `s` (`panel`) MUST esconder e mostrar o painel, e a chave `panel` de `[tui]` MUST definir se ele abre visível.

#### Scenario: Painel visível
- **WHEN** o terminal tem 120 colunas e o usuário abre a TUI
- **THEN** o painel aparece à direita, e a sessão da aba ativa tem 86 colunas

#### Scenario: Esconder
- **WHEN** o painel está visível e o usuário digita Ctrl+b s
- **THEN** o painel some, e a sessão volta a ter a largura toda

#### Scenario: Terminal estreito
- **WHEN** o terminal tem 80 colunas
- **THEN** o painel não aparece

### Requirement: Proteção no painel
A seção "Proteção" MUST mostrar, para o último lançamento da aba ativa: o nível de proteção, com a ABI do Landlock; a camada de escrita; a pasta de trabalho e as pastas extras, cada uma em somente leitura ou com escrita; a GPU e o KVM; a escrita no clipboard; as variáveis de `pass-env`, com o aviso de que ficam ao alcance do agente; os tetos de memória e de processos; e o estado da rede.

#### Scenario: Pasta extra e variável
- **WHEN** a sessão da aba ativa foi lançada com `--ro ~/docs` e `--pass-env OPENROUTER_API_KEY`
- **THEN** o painel mostra `~/docs` em somente leitura e `OPENROUTER_API_KEY` com o aviso

### Requirement: Recursos no painel
A seção "Recursos" MUST mostrar a memória em uso, o pico e o teto, os processos em uso e o teto, o CPU usado, as mortes por falta de memória e, quando o Docker privado da pasta está rodando, os contêineres e a memória dele, atualizados a cada segundo.

#### Scenario: Consumo atualizado
- **WHEN** um programa da aba ativa passa a usar 200 MB
- **THEN** em até dois segundos o painel mostra a memória em uso maior que 200 MB
