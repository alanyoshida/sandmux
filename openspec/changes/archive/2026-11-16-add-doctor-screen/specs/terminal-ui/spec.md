## ADDED Requirements

### Requirement: Diagnóstico na página de configurações
A página de configurações MUST ter a seção "Diagnóstico", depois do Cofre, com as verificações do `sandmux doctor`: uma linha por verificação, com `✓`, `✗` ou `·`, agrupadas, e a causa e a correção da linha escolhida embaixo. A seção MUST rodar as verificações ao entrar, e `r` MUST rodá-las de novo. O nome da seção MUST mostrar quantas falharam. A ação `doctor` (padrão `D`) MUST abrir a página na seção.

#### Scenario: Ver uma falha
- **WHEN** a máquina não tem o binário `registry` e o usuário digita Ctrl+b `D`
- **THEN** a página abre no Diagnóstico, a lateral mostra "Diagnóstico ✗1", a linha do cache de imagens tem `✗`, e, escolhida, mostra embaixo como instalar o `registry`

#### Scenario: Rodar de novo
- **WHEN** o arquivo com uma chave real é apagado fora da TUI e o usuário aperta `r` na seção
- **THEN** a linha da chave some

#### Scenario: Saída do terminal igual
- **WHEN** o usuário roda `sandmux doctor` no terminal
- **THEN** a saída é a mesma de antes deste change
