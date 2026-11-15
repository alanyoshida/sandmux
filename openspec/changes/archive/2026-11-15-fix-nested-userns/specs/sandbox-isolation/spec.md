## MODIFIED Requirements

### Requirement: User namespaces aninhados permitidos, sem montagem
Um processo confinado MUST poder criar user namespaces próprios e mapear neles o próprio uid, para que programas que se isolam continuem funcionando. Dentro de um namespace aninhado, o processo MUST continuar sem conseguir montar nada nem alcançar o que o sandbox esconde.

#### Scenario: Navegador headless
- **WHEN** um navegador headless que cria o próprio isolamento é iniciado dentro do sandbox
- **THEN** ele inicia

#### Scenario: Montagem em namespace aninhado
- **WHEN** um processo confinado cria um user namespace e tenta montar um sistema de arquivos
- **THEN** a montagem falha

#### Scenario: Chrome com o sandbox dele
- **WHEN** o `google-chrome` é iniciado no sandbox, sem `--no-sandbox`
- **THEN** ele inicia com o sandbox de namespaces e seccomp dele
