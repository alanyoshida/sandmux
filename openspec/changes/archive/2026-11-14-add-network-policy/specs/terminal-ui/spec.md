# Spec Delta

## MODIFIED Requirements

### Requirement: Rede no painel e na barra
A seção "Rede" do painel MUST mostrar o modo de rede e as últimas conexões da aba ativa, com domínio (ou "sem domínio"), volume e bloqueio, destacando como "novo" o domínio visto pela primeira vez naquela pasta de trabalho. Uma aba inativa MUST ganhar na barra `•` quando a sessão acessa um domínio novo e `!` quando a sessão tem um bloqueio, até ficar ativa.

#### Scenario: Domínio novo na aba ativa
- **WHEN** o agente da aba ativa acessa pela primeira vez um domínio naquela pasta de trabalho
- **THEN** a seção Rede mostra o domínio marcado como novo

#### Scenario: Domínio novo em outra aba
- **WHEN** o agente de uma aba inativa acessa pela primeira vez um domínio naquela pasta de trabalho
- **THEN** a aba ganha uma marca na barra, que some quando ela fica ativa

#### Scenario: Bloqueio em outra aba
- **WHEN** o agente de uma aba inativa tem uma conexão bloqueada
- **THEN** a aba ganha `!` na barra, que some quando ela fica ativa

## ADDED Requirements

### Requirement: Tela de rede
O atalho `a` (`network`) MUST abrir a tela de rede da aba ativa, com os acessos recentes (mais novos primeiro) e as regras efetivas. Na tela, o usuário MUST poder:

- negar o destino de um acesso, ou liberar o de um bloqueio, escolhendo a camada: a aba, o perfil do projeto ou o global;
- ligar e desligar um item dos catálogos ou da allowlist;
- trocar o modo de rede da aba.

Cada mudança MUST valer na conexão seguinte da sessão.

#### Scenario: Negar um acesso novo
- **WHEN** o usuário abre a tela de rede, escolhe um acesso a `host.sandmux.internal` e o nega na aba
- **THEN** o acesso seguinte da sessão a `host.sandmux.internal` é bloqueado

#### Scenario: Liberar no projeto
- **WHEN** a aba está no modo `custom`, o usuário escolhe um bloqueio de `host.sandmux.internal` e o libera no projeto
- **THEN** o perfil do projeto ganha `allow = ["host.sandmux.internal"]`, e o acesso seguinte passa

#### Scenario: Trocar o modo
- **WHEN** o usuário troca o modo da aba de `aberta` para `custom` na tela de rede
- **THEN** a conexão seguinte da sessão já segue o modo `custom`
