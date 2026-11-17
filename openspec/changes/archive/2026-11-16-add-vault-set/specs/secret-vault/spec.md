## ADDED Requirements

### Requirement: Troca do valor de um segredo
`sandmux vault set <nome>` MUST pedir só o valor novo, sem eco, e trocar o valor do segredo de token, mantendo o falso, os domínios e as ferramentas. Um nome que não existe, uma chave SSH ou uma conexão de banco MUST ser recusados, com o comando certo. Na seção Cofre da TUI, a tecla `v` MUST fazer o mesmo com o segredo escolhido. O valor novo MUST valer na próxima conexão, sem reiniciar a aba.

#### Scenario: Chave vencida
- **WHEN** o segredo `openrouter` tem o falso `sandmux-fake-abc` e o usuário roda `sandmux vault set openrouter` com o valor novo
- **THEN** o cofre guarda o valor novo, `sandmux vault list` mostra o mesmo falso e os mesmos domínios, e a próxima conexão ao `openrouter.ai` leva o valor novo

#### Scenario: Nome que não existe
- **WHEN** o usuário roda `sandmux vault set naoexiste`
- **THEN** o comando falha e indica `sandmux vault add naoexiste`

#### Scenario: Pela TUI
- **WHEN** na seção Cofre o usuário escolhe `lmstudio`, aperta `v` e digita o valor novo
- **THEN** o valor não aparece na tela, e o cofre guarda o valor novo com o mesmo falso
