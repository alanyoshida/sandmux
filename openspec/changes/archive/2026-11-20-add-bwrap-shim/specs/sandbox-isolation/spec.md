## MODIFIED Requirements

### Requirement: User namespaces aninhados permitidos, sem montagem
Um processo confinado MUST poder criar user namespaces próprios e mapear neles o próprio uid, para que programas que se isolam continuem funcionando. Dentro de um namespace aninhado, o processo MUST continuar sem conseguir montar nada nem alcançar o que o sandbox esconde. Um `bwrap` no `PATH` do sandbox MUST executar o comando pedido sem montar, com as restrições do pedido que não precisam de montagem.

#### Scenario: Navegador headless
- **WHEN** um navegador headless que cria o próprio isolamento é iniciado dentro do sandbox
- **THEN** ele inicia

#### Scenario: Montagem em namespace aninhado
- **WHEN** um processo confinado cria um user namespace e tenta montar um sistema de arquivos
- **THEN** a montagem falha

#### Scenario: Chrome com o sandbox dele
- **WHEN** o `google-chrome` é iniciado no sandbox, sem `--no-sandbox`
- **THEN** ele inicia com o sandbox de namespaces e seccomp dele

#### Scenario: Sandbox do bash do Claude Code
- **WHEN** o Claude Code roda na aba com o sandbox do bash dele ligado e executa um comando
- **THEN** o comando roda, e uma escrita fora das pastas que o Claude Code liberou falha

#### Scenario: bwrap com binds
- **WHEN** um processo confinado roda `bwrap --ro-bind / / --bind $PWD $PWD -- sh -c 'touch $PWD/x; touch ~/y'`
- **THEN** `$PWD/x` é criado e `~/y` não
