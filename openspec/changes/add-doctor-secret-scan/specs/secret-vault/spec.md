## ADDED Requirements

### Requirement: Chaves reais nos HOMEs sintéticos
`sandmux doctor` MUST procurar, nos arquivos da tabela de importação de cada harness dentro do HOME sintético, valores de credencial que não são falsos do sandmux, com as mesmas regras da importação, e listar cada um com o harness, o arquivo, o provedor (quando há), a chave e a linha, sem o valor. `sandmux doctor --fix` MUST, arquivo a arquivo, guardar cada chave no cofre com um valor falso novo e o domínio do provedor (ou perguntado), e só depois regravar o arquivo com os falsos. Se o cofre recusar uma chave, o arquivo MUST ficar como estava.

#### Scenario: Chave colada no opencode do sandbox
- **WHEN** o usuário conectou o OpenRouter pelo opencode dentro do sandbox, e o `auth.json` do HOME sintético tem a chave real
- **THEN** o `sandmux doctor` lista `opencode: .local/share/opencode/auth.json`, provedor `openrouter`, chave `key`, sem mostrar o valor, e indica `sandmux doctor --fix`

#### Scenario: Mover para o cofre
- **WHEN** o usuário roda `sandmux doctor --fix` e confirma
- **THEN** o cofre ganha o segredo com o domínio `openrouter.ai` e um valor falso, o `auth.json` passa a ter o falso, e o real não fica em nenhum arquivo do HOME sintético
