## MODIFIED Requirements

### Requirement: Alterar a configuração pela linha de comando
`sandmux config set <chave> <valor>` e `unset <chave>` MUST alterar uma chave de liga e desliga; `add <chave> <item>` e `remove <chave> <item>` MUST alterar um item de lista. Cada comando MUST exigir um alvo explícito: `--global`, `--profile <nome>` ou `--project [<pasta>]`, que sem pasta usa a atual. O comando MUST validar o resultado antes de gravar e MUST recusar o que tornaria o arquivo inválido.

#### Scenario: Ligar o KVM num perfil
- **WHEN** o usuário roda `sandmux config set kvm true --profile vm` e não existe `[profile.vm]`
- **THEN** o arquivo passa a ter `[profile.vm]` com `kvm = true`

#### Scenario: Sem alvo
- **WHEN** o usuário roda `sandmux config set kvm true`
- **THEN** o comando termina com erro pedindo `--global`, `--profile` ou `--project`, e o arquivo não muda

#### Scenario: Projeto da pasta atual
- **WHEN** o usuário roda `sandmux config add ro ~/docs --project` na pasta `/home/ana/proj`
- **THEN** o arquivo passa a ter `[project."/home/ana/proj"]` com `~/docs` na lista `ro`

#### Scenario: Escrita recusada
- **WHEN** o usuário roda `sandmux config add rw ~/.config --global`
- **THEN** o comando termina com erro e o arquivo não muda

#### Scenario: Chaves fora das camadas
- **WHEN** a TUI grava `theme = "dracula"` em `[tui]`, `chat-model` em `[assistant]` ou `net-log-days = 30` em `[global]`
- **THEN** o editor grava só essa chave, preserva o resto do arquivo e recusa um valor que deixaria o arquivo inválido
