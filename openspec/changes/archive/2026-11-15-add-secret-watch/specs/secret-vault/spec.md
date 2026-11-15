## ADDED Requirements

### Requirement: Varredura das chaves reais pelo daemon
O daemon MUST procurar chaves reais nos arquivos que o `sandmux doctor` lê nos HOMEs sintéticos, com as mesmas regras, nestes momentos:

- quando um desses arquivos é gravado;
- ao subir;
- a cada 60 segundos;
- logo depois de o cofre ser aberto.

#### Scenario: Gravação perdida
- **WHEN** uma chave real está num arquivo do HOME sintético e nenhum aviso de gravação chegou ao daemon
- **THEN** a varredura periódica a acha em até 60 segundos

### Requirement: Movimento automático das chaves reais
Com o cofre aberto e o domínio de cada chave conhecido pelas mesmas regras do `doctor --fix`, o daemon MUST guardar cada chave do arquivo no cofre, com um falso novo e o domínio, e só depois regravar o arquivo com os falsos. Se o cofre recusar uma chave, se uma chave do arquivo não tiver domínio conhecido, ou se o arquivo mudou desde a leitura, o arquivo MUST ficar como estava. Os valores reais MUST NOT aparecer no registro do daemon nem nas mensagens.

#### Scenario: Chave conectada pelo opencode
- **WHEN** o cofre está aberto e o opencode do sandbox grava a chave real do OpenRouter no `auth.json`
- **THEN** em poucos segundos o cofre tem o segredo com o domínio `openrouter.ai`, o `auth.json` tem o falso, e a aba diz que foi movida uma chave e que precisa ser reiniciada

### Requirement: Chaves esperando o cofre ou um domínio
Quando uma chave real não pode ser movida sozinha, o daemon MUST deixar o arquivo como está e informar, nas abas do harness, quantas chaves esperam o cofre (trancado ou ausente) e quantas esperam um domínio. As que esperam o cofre MUST ser movidas logo depois de o cofre ser aberto. As que esperam um domínio continuam para o `sandmux doctor --fix`.

#### Scenario: Cofre trancado
- **WHEN** o cofre está trancado e o hermes grava uma chave real no `.env` do HOME sintético
- **THEN** o arquivo fica como está, a aba do hermes mostra uma chave esperando o cofre, e, quando o cofre é aberto, a chave vai para o cofre e o arquivo passa a ter o falso

#### Scenario: Domínio desconhecido
- **WHEN** uma chave real tem um nome e um provedor que não indicam o domínio
- **THEN** o arquivo fica como está, a aba mostra uma chave esperando um domínio, e o `sandmux doctor --fix` pergunta o domínio
