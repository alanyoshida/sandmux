## ADDED Requirements

### Requirement: Namespaces aninhados nas aberturas
O supervisor MUST abrir o `uid_map`, `gid_map`, `projid_map` e `setgroups` do próprio processo no user namespace pai do dele, para que a gravação do mapeamento siga as regras do kernel como fora do sandbox. O `/proc/self` MUST ser o do processo também quando ele está num pid namespace criado dentro do sandbox.

#### Scenario: Mapeamento de ids
- **WHEN** um processo do sandbox cria um user namespace e grava o próprio uid no `/proc/self/uid_map`
- **THEN** a gravação funciona, como fora do sandbox

#### Scenario: Sem namespace próprio
- **WHEN** um processo que não criou user namespace tenta gravar no `/proc/self/uid_map`
- **THEN** a gravação é recusada

#### Scenario: Pid namespace aninhado
- **WHEN** um processo num pid namespace criado dentro do sandbox abre `/proc/self/status`
- **THEN** ele lê o próprio status, e não o de outro processo
