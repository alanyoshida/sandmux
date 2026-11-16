## ADDED Requirements

### Requirement: Reiniciar aba viva
A ação `restart` (Ctrl+b `r`) numa aba viva MUST pedir confirmação. Confirmada, a TUI MUST pedir ao daemon o reinício forçado e anexar a aba relançada. Recusada, nada muda.

#### Scenario: Confirmação
- **WHEN** a aba ativa está viva e o usuário aperta Ctrl+b `r` e `s`
- **THEN** o harness é encerrado e relançado na mesma aba, com a configuração de agora

#### Scenario: Recusa
- **WHEN** o usuário aperta Ctrl+b `r` e `n` numa aba viva
- **THEN** a aba continua como estava

### Requirement: Aviso das abas vivas na configuração
Uma mudança gravada pela tela de configurações num item que vale no lançamento MUST dizer quantas abas vivas continuam com o valor antigo e que Ctrl+b `r` as reinicia. O `sandmux config set`, `unset`, `add` e `remove` MUST mostrar o mesmo aviso quando o daemon tem sessões vivas.

#### Scenario: Janela ligada com abas vivas
- **WHEN** há duas abas vivas e o usuário liga `window` na tela de configurações
- **THEN** a página diz que 2 abas vivas continuam sem a mudança e que Ctrl+b `r` reinicia a aba

## MODIFIED Requirements

### Requirement: Proteção no painel
A seção "Proteção" MUST mostrar, para o último lançamento da aba ativa: o nível de proteção, com a ABI do Landlock; a camada de escrita; a pasta de trabalho e as pastas extras, cada uma em somente leitura ou com escrita; a GPU e o KVM; a janela, restrita ou direta; a escrita no clipboard; as variáveis de `pass-env`, com o aviso de que ficam ao alcance do agente; os tetos de memória e de processos; e o estado da rede.

#### Scenario: Pasta extra e variável
- **WHEN** a sessão da aba ativa foi lançada com `--ro ~/docs` e `--pass-env OPENROUTER_API_KEY`
- **THEN** o painel mostra `~/docs` em somente leitura e `OPENROUTER_API_KEY` com o aviso

#### Scenario: Janela
- **WHEN** a sessão da aba ativa foi lançada com `window = true`
- **THEN** o painel mostra a janela ligada, e com `window = "direct"`, a janela direta
