## MODIFIED Requirements

### Requirement: Histórico em memória
O daemon MUST guardar as últimas 10.000 linhas que saíram da tela principal de cada sessão, valor ajustável por uma opção do `sandmuxd`. Linhas mais antigas MUST ser descartadas. Com `[global] screen-history` desligado, que é o padrão, o histórico e a saída das sessões MUST NOT ser gravados em disco.

#### Scenario: Limite de linhas
- **WHEN** uma sessão imprime 12.000 linhas com o limite padrão
- **THEN** ao anexar, o histórico recebido tem as 10.000 linhas mais recentes

#### Scenario: Nada em disco
- **WHEN** `screen-history` está desligado e uma sessão imprime um texto marcador
- **THEN** o marcador não aparece em nenhum arquivo do diretório de dados, do `$XDG_RUNTIME_DIR` ou da pasta de estado do sandmux

## ADDED Requirements

### Requirement: Histórico de tela cifrado em disco
Com `[global] screen-history = true`, o daemon MUST gravar continuamente a saída de cada sessão num arquivo da pasta de estado, cifrado com uma chave guardada no cofre. Sem cofre, nada MUST ser gravado. O arquivo MUST NOT conter a saída em claro. Com o cofre trancado, a saída MUST esperar na memória, com um teto por sessão, e ser gravada quando o cofre abrir.

#### Scenario: Gravado cifrado
- **WHEN** a opção está ligada, o cofre está aberto e uma sessão imprime um texto marcador
- **THEN** em poucos segundos existe o arquivo do histórico da sessão, e o marcador não aparece em claro em nenhum arquivo

### Requirement: Histórico de volta depois do reinício
Com o histórico em disco ligado, uma sessão relançada depois do reinício do daemon MUST ter de volta o histórico gravado, antes da saída nova. Com o cofre trancado, o histórico MUST voltar quando o cofre for aberto.

#### Scenario: Depois de daemon stop
- **WHEN** a opção está ligada, uma sessão imprimiu um texto marcador, e o daemon para e sobe de novo com o cofre aberto
- **THEN** ao anexar, o marcador está no histórico da sessão

#### Scenario: Cofre trancado
- **WHEN** o daemon sobe de novo com o cofre trancado
- **THEN** o marcador não está no histórico até o cofre ser aberto, e depois está

### Requirement: Limpeza do histórico em disco
Fechar uma sessão MUST apagar o arquivo do histórico dela. Desligar `screen-history` MUST apagar todos os arquivos do histórico.

#### Scenario: Fechar a aba
- **WHEN** o usuário encerra a sessão com `sandmux kill`
- **THEN** o arquivo do histórico dela some em poucos segundos
