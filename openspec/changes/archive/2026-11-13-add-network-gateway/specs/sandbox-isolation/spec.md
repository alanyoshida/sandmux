# Spec Delta

## REMOVED Requirements

### Requirement: Rede do host sem filtro nesta fatia
**Reason**: A rede do sandbox passa a ter namespace próprio e gateway, com registro e bloqueios, na capacidade `network-gateway`.
**Migration**: O modo de rede padrão, aberta, mantém o acesso à internet e à rede local, agora registrado. Para nenhuma rede, `--network sem-rede` ou `network = "sem-rede"`.
