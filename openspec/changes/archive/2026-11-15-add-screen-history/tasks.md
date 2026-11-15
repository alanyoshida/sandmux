## 1. Cofre e configuração

- [x] 1.1 `history_key` no conteúdo do cofre (criada no primeiro uso), cifra e decifra dos registros com AAD da aba, registro cortado ignorado; testes unitários
- [x] 1.2 `[global] screen-history` (padrão desligado) na configuração e no item da seção Geral

## 2. Daemon

- [x] 2.1 Buffer da saída por sessão (teto de 2 MiB) e a thread de gravação: a cada 2 s grava os buffers cifrados, compacta acima de 8 MiB, lê a opção a cada 10 s, apaga os arquivos ao desligar e os sem sessão
- [x] 2.2 Histórico no lançamento (com a chave) com o separador; marca de pendente sem a chave; reconstrução no desbloqueio com o redesenho dos clientes

## 3. Testes e documentação

- [x] 3.1 Testes: desligado nada em disco; ligado gravado e sem texto em claro; volta depois de `daemon stop`; com o cofre trancado só no desbloqueio; some ao fechar a aba e ao desligar
- [x] 3.2 Documentação: `docs/sessions.md`, `docs/config.md`, `docs/tui.md` e os pares em inglês
