## Why

Item "Histórico de tela em disco" da lista depois da v1, escolhido pelo usuário (2026-11-15): guardar a saída das abas em arquivo, para ela sobreviver ao reinício do daemon.

Hoje a regra é que a saída das abas nunca vai para o disco, porque ela pode mostrar segredos que o agente imprimiu. Por isso o histórico some a cada reinício do daemon, inclusive numa atualização.

Decisões do usuário (2026-11-15):

- **Proteção:** cifrado com uma chave guardada no cofre. Sem cofre, nada é gravado; o histórico só volta com o cofre aberto.
- **Ativação:** desligado por padrão, por opção (`[global] screen-history = true`, também nas configurações da TUI). Desligado, a regra antiga continua valendo.
- **Quando grava:** continuamente, para sobreviver também a uma queda.

## What Changes

- **Opção `[global] screen-history`** (padrão `false`), também na seção Geral das configurações.
- **Gravação contínua e cifrada:** com a opção ligada e o cofre aberto, a saída de cada aba vai para `<estado>/history/<id>.bin`, a cada 2 segundos:
  - cada trecho é cifrado com XChaCha20-Poly1305 e uma chave aleatória guardada no cofre, ligada ao número da aba;
  - o arquivo tem modo 600, numa pasta 700;
  - acima de 8 MiB, ele é compactado para os 4 MiB mais recentes.
- **Cofre trancado:** a saída espera na memória, até 2 MiB por aba, e é gravada quando o cofre abre. O que passar disso se perde, e o registro do daemon diz isso, sem o conteúdo.
- **Restauração:** quando a aba é lançada com o cofre aberto, o histórico gravado é carregado antes da saída nova, e um separador marca o reinício. Com o cofre trancado, o histórico volta quando ele é aberto, e os clientes anexados recebem a tela redesenhada com o histórico.
- **Limpeza:**
  - fechar a aba apaga o arquivo;
  - desligar a opção apaga todos os arquivos;
  - trocar a senha do cofre não muda a chave do histórico.
- **A regra "nada em disco"** continua valendo com a opção desligada. A spec registra a exceção, como a da conversa do chat.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `terminal-sessions`: o histórico em memória ganha a gravação cifrada e opcional em disco.

## Impact

- **`sandmux-vault`:** a chave do histórico no conteúdo do cofre (`history_key`, criada no primeiro uso), e a cifra e decifra dos trechos.
- **`sandmux-policy`:** a chave `screen-history` em `[global]`.
- **Daemon:**
  - o buffer da saída por aba;
  - a thread de gravação, com a compactação, a limpeza dos arquivos sem aba e a leitura da opção;
  - o carregamento no lançamento e a reconstrução no desbloqueio.
- **TUI:** o item na seção Geral.
- **Testes:**
  - desligado, nada em disco (o teste atual);
  - ligado: o arquivo existe e não contém o texto em claro, o histórico volta depois de `daemon stop`, volta com o cofre trancado só no desbloqueio, e some ao fechar a aba e ao desligar a opção;
  - teste unitário da cifra dos trechos (ida e volta, chave errada, trecho cortado no fim).
- **Documentação:** `docs/sessions.md`, `docs/config.md`, `docs/tui.md` e os pares em inglês.
