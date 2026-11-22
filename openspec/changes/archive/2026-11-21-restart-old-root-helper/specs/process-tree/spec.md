## ADDED Requirements

### Requirement: Pai pelo /proc quando o evento não o traz
Quando um evento de execução do auxiliar root chega sem o pai (um auxiliar de antes da 0.1.32), o daemon MUST prender o nó ao pai lido no `/proc/<pid>/stat` do processo. Só quando o processo já não existe no `/proc` o nó MUST ficar sem pai.

#### Scenario: Auxiliar antigo
- **WHEN** o auxiliar manda a execução de um `sleep` sem o pai, e no `/proc` o pai do `sleep` é o `bash` que já está na árvore
- **THEN** a árvore mostra o `sleep` como filho do `bash`, e não na raiz

#### Scenario: Processo que já terminou
- **WHEN** o evento chega sem o pai e o processo já não está no `/proc`
- **THEN** o nó fica sem pai, e sai da árvore no próximo pedido

## MODIFIED Requirements

### Requirement: Aviso de auxiliar antigo na seção Processos
Quando o auxiliar root em execução é de uma versão anterior à do sandmux, a seção "Processos" MUST mostrar, acima da árvore ou do motivo de ela não existir, um aviso com as duas versões e a correção: `sandmux setup`, que reinicia o auxiliar, ou `sudo systemctl restart sandmux-helper.service`. A seção MUST conferir a versão ao abrir e ao reler. Sem auxiliar, ou com o auxiliar na mesma versão ou numa mais nova, a seção MUST NOT mostrar o aviso.

#### Scenario: Auxiliar antigo
- **WHEN** o auxiliar em execução responde uma versão anterior à do sandmux e o usuário abre a seção Processos
- **THEN** a seção mostra o aviso com as duas versões, `sandmux setup` e `sudo systemctl restart sandmux-helper.service`

#### Scenario: Auxiliar em dia
- **WHEN** o auxiliar em execução responde a mesma versão do sandmux e o usuário abre a seção Processos
- **THEN** a seção não mostra o aviso

#### Scenario: Reler depois de atualizar
- **WHEN** o aviso está na tela, o auxiliar passa a responder a versão do sandmux e o usuário aperta `r`
- **THEN** o aviso some
