## ADDED Requirements

### Requirement: Chat no modal de nova aba
O modal de nova aba MUST oferecer `chat` depois dos harnesses. Com `chat` escolhido, Enter MUST abrir o modal de chat, com o modelo e a mensagem, e a dica do modal MUST dizer que a pasta e o perfil não valem para o chat.

#### Scenario: Escolher chat
- **WHEN** o usuário abre o modal de nova aba, passa com → até `chat` e aperta Enter
- **THEN** o modal de chat abre, com o modelo padrão de `[assistant]`
