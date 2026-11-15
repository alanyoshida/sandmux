## Context

O `vt100::Parser` de cada sessão guarda a tela e o histórico na memória. `reset_terminal` cria um parser novo a cada lançamento, e `replay` manda o histórico e a tela a um cliente que se anexa. A thread leitora da sessão filtra a saída (OSC 52) e a passa ao parser e aos clientes. O cofre cifra o conteúdo com XChaCha20-Poly1305 e uma chave derivada da senha (Argon2id).

## Goals / Non-Goals

**Goals:**
- O histórico sobrevive ao reinício do daemon e a uma queda, sem nunca ficar em claro no disco.
- Desligado, que é o padrão, nada muda.

**Non-Goals:**
- Guardar a tela alternativa (opencode e outros em tela cheia). O que se guarda é a saída crua; a reconstrução refaz o que o terminal mostraria.
- Histórico de abas já fechadas.

## Decisions

- **Chave do histórico no cofre:** 32 bytes aleatórios em `Contents.history_key`, criados no primeiro uso. Fica fora da senha, então `vault passwd` não mexe no histórico. Ela só existe na memória do daemon com o cofre aberto.
- **Formato:** `<estado>/history/<id>.bin`, só de acréscimo, com registros `nonce(24) | tamanho(u32 LE) | cifra`. O AAD é `sandmux-history:<id>`, e um arquivo trocado de aba não decifra. Um registro cortado no fim, depois de uma queda, é ignorado.
- **Saída crua, não linhas:** o que se grava é o que a sessão recebeu depois do filtro. Para reconstruir, basta passá-lo a um parser novo, do mesmo tamanho e com o mesmo teto de linhas.
- **Gravação:**
  - a thread leitora acrescenta a saída ao buffer da sessão (só com a opção ligada);
  - a thread `historico` grava os buffers a cada 2 segundos;
  - acima de 8 MiB, ela decifra, guarda os 4 MiB finais (o corte pode partir uma sequência de escape, o que o parser tolera) e regrava com troca atômica;
  - com o cofre trancado, o buffer fica na memória com teto de 2 MiB, descartando o mais antigo.
- **A opção** é lida do arquivo de configuração pela thread a cada 10 segundos, porque é rara de mudar. Desligada: o buffer não cresce, e os arquivos são apagados. A thread também apaga os arquivos sem sessão (aba fechada).
- **Lançamento:** em `attach_process`, depois de `reset_terminal`:
  - com a chave na memória, o arquivo é decifrado e passado ao parser novo, seguido de um separador (`── sandmux: a aba voltou ──`);
  - sem a chave, a sessão fica marcada como histórico pendente.
- **Desbloqueio:**
  - cada sessão pendente tem o parser refeito: o arquivo, que agora inclui o buffer gravado, mais um separador e a tela atual;
  - os clientes anexados recebem um "limpar e redesenhar" (`ESC[H ESC[2J` mais o `replay`).
- **Ordem na subida:** a restauração relança as sessões antes da tentativa do keyring. Com o keyring, o desbloqueio vem logo depois e reconstrói as sessões pendentes; sem keyring, isso acontece no desbloqueio manual.

## Risks / Trade-offs

- [Histórico em disco é dado sensível] → Cifrado, opcional e desligado por padrão, apagado ao fechar a aba e ao desligar. A chave está no cofre.
- [Até 2 segundos de saída perdidos numa queda] → Aceito; é o preço de não gravar a cada byte.
- [A reconstrução no desbloqueio redesenha a tela dos clientes] → Uma vez por sessão, só no desbloqueio.
