# Spec Delta

## MODIFIED Requirements

### Requirement: Sistema em somente leitura
As pastas de sistema do host (`/usr`, `/bin`, `/sbin`, `/lib*`, `/opt`, `/etc`, `/sys`) MUST aparecer em somente leitura. `/proc`, `/tmp`, `/run` e `/var/tmp` MUST ser novos e privados do sandbox. Nenhuma outra pasta do host MUST existir dentro do sandbox por padrão.

#### Scenario: Escrita no sistema
- **WHEN** um processo confinado tenta criar ou alterar um arquivo em `/usr` ou `/etc`
- **THEN** a operação falha

#### Scenario: Arquivos temporários do host
- **WHEN** um processo confinado lista `/tmp` e `/run`
- **THEN** não aparece nenhum arquivo ou socket do host

#### Scenario: Pasta fora da visão
- **WHEN** um processo confinado tenta ler `/media`, `/srv` ou `/root`, ou lista `/mnt`
- **THEN** os três primeiros não existem, e `/mnt` só tem o `sandmux` do sandmux (as pastas novas ao vivo); nada do `/mnt` do host aparece
