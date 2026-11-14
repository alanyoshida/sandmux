# Spec Delta

## MODIFIED Requirements

### Requirement: Diagnóstico de suporte da máquina
`sandmux doctor` MUST mostrar o resultado de cada verificação de suporte (user namespaces sem privilégio, versão do Landlock, camada de escrita com overlay, limites de recurso) e, para cada falha, a causa e o que fazer. O comando MUST terminar com código diferente de zero quando o sandbox não pode ser lançado.

#### Scenario: Máquina com suporte completo
- **WHEN** todas as verificações passam
- **THEN** a saída lista cada verificação como atendida e o comando termina com código zero

#### Scenario: Restrição de user namespace por AppArmor
- **WHEN** a distro restringe user namespaces sem privilégio por AppArmor
- **THEN** a saída explica a causa, mostra o perfil AppArmor necessário para o lançador e o comando que o instala, e termina com código diferente de zero

#### Scenario: Camada de escrita indisponível
- **WHEN** o overlay em user namespace não funciona na máquina
- **THEN** a saída avisa que caches e toolchains vão funcionar no modo de recuo, e o comando termina com código zero

#### Scenario: Limites de recurso indisponíveis
- **WHEN** o systemd de usuário não responde ou não delega os controladores `memory` e `pids`
- **THEN** a saída avisa que as sessões vão rodar sem teto de memória e de processos, explica a causa e o que fazer, e o comando termina com código zero
