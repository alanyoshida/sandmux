# Spec Delta

## MODIFIED Requirements

### Requirement: Painel lateral
A TUI MUST mostrar à direita um painel de 34 colunas com as seções "Proteção", "Recursos" e "Rede" da aba ativa, quando o terminal tem pelo menos 100 colunas e o painel não foi escondido. O atalho `s` (`panel`) MUST esconder e mostrar o painel, e a chave `panel` de `[tui]` MUST definir se ele abre visível.

#### Scenario: Painel visível
- **WHEN** o terminal tem 120 colunas e o usuário abre a TUI
- **THEN** o painel aparece à direita, e a sessão da aba ativa tem 86 colunas

#### Scenario: Esconder
- **WHEN** o painel está visível e o usuário digita Ctrl+b s
- **THEN** o painel some, e a sessão volta a ter a largura toda

#### Scenario: Terminal estreito
- **WHEN** o terminal tem 80 colunas
- **THEN** o painel não aparece

## ADDED Requirements

### Requirement: Rede no painel e na barra
A seção "Rede" do painel MUST mostrar o modo de rede e as últimas conexões da aba ativa, com domínio (ou "sem domínio") e volume, destacando como "novo" o domínio visto pela primeira vez naquela pasta de trabalho. Uma aba inativa cuja sessão acessa um domínio novo MUST ganhar uma marca na barra até ficar ativa.

#### Scenario: Domínio novo na aba ativa
- **WHEN** o agente da aba ativa acessa pela primeira vez um domínio naquela pasta de trabalho
- **THEN** a seção Rede mostra o domínio marcado como novo

#### Scenario: Domínio novo em outra aba
- **WHEN** o agente de uma aba inativa acessa pela primeira vez um domínio naquela pasta de trabalho
- **THEN** a aba ganha uma marca na barra, que some quando ela fica ativa
