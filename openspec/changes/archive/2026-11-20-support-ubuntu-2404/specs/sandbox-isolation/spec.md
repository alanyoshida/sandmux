## MODIFIED Requirements

### Requirement: Falha fechada sem suporte do kernel
O lançador MUST verificar, antes de executar o comando, que a máquina permite user namespaces sem privilégio e oferece o escopo de sockets abstratos e de sinais e a restrição de `ioctl` em dispositivos: pelo Landlock ABI 6 ou, com o ABI 4 ou 5, pelo auxiliar root com o BPF LSM ativo. Faltando qualquer um, o lançador MUST recusar o lançamento. Não existe opção para lançar sem confinamento.

#### Scenario: Landlock ausente ou antigo
- **WHEN** o kernel não oferece Landlock com escopo de sockets abstratos e o auxiliar root não pode completar o que falta
- **THEN** o lançamento é recusado, a mensagem diz qual suporte falta e nenhum comando é executado

#### Scenario: Kernel 6.8 com o auxiliar
- **WHEN** o kernel tem o Landlock ABI 4, o `bpf` está entre os LSMs ativos e o `sandmux-helper` está instalado
- **THEN** o comando roda, e um socket abstrato do host, um sinal a um processo de fora e um `ioctl` num dispositivo de fora da lista são negados como num kernel com o ABI 6

#### Scenario: Kernel 6.8 sem o BPF LSM
- **WHEN** o kernel tem o Landlock ABI 4 e o `bpf` não está entre os LSMs ativos
- **THEN** o lançamento é recusado, e a mensagem aponta para `sandmux doctor`

#### Scenario: User namespace bloqueado
- **WHEN** a máquina não permite criar user namespace sem privilégio
- **THEN** o lançamento é recusado e a mensagem aponta para `sandmux doctor`

#### Scenario: Falha ao montar o confinamento
- **WHEN** qualquer etapa da montagem do confinamento falha, inclusive o registro no auxiliar root
- **THEN** o comando não é executado e o lançador termina com erro
