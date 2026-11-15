## ADDED Requirements

### Requirement: Lista de modelos dos provedores
O sandmux MUST obter os modelos de cada provedor configurado por `GET <url>/models`, dentro do sandbox da aba de chat, com a chave trocada pelo gateway, e MUST devolver a lista como `provedor/modelo`. Um provedor que falha MUST ficar de fora, com o motivo nos avisos; a lista MUST falhar só se nenhum provedor responder.

#### Scenario: Provedor responde
- **WHEN** o provedor `openrouter` responde `/models` com `deepseek/deepseek-v4.1-flash-1017`
- **THEN** a lista tem `openrouter/deepseek/deepseek-v4.1-flash-1017`

### Requirement: Seletor de modelo
Onde se escolhe o modelo do assistente, a TUI MUST oferecer um seletor com a lista dos provedores e filtro por digitação:
- os itens "Modelo do chat", "Modelo de comandos" e "Modelos sugeridos" da página de configurações;
- o campo do modelo dos modais de chat e de comando;
- o `/modelo` sem argumento na aba de chat.

Enter MUST escolher o item marcado e, sem item, o texto digitado.

#### Scenario: Escolher no modal de chat
- **WHEN** no campo do modelo do modal de chat o usuário aperta →, digita `flash` e Enter
- **THEN** o campo passa a ter o modelo escolhido da lista

#### Scenario: Gravar na configuração
- **WHEN** na página de configurações o usuário escolhe um modelo para "Modelo do chat"
- **THEN** o `config.toml` passa a ter `chat-model` com esse modelo em `[assistant]`
