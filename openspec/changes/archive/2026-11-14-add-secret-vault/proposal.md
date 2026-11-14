# Proposal

## Why

A regra do projeto é que nenhum segredo real entra no sandbox, mas até agora não existe onde guardar um segredo fora dele. Hoje a única saída é o `--pass-env`, que põe o valor dentro do sandbox e some num reboot: uma sessão com `--pass-env` fica parada até o usuário reiniciá-la de um terminal com a variável. A etapa 5 do roadmap troca isso por chaves falsas no sandbox e a troca pela real no tráfego (changes 12, 13 e 25), e todas dependem de um cofre. Este change faz o cofre.

## What Changes

- **Cofre em arquivo cifrado com senha**, em `$XDG_DATA_HOME/sandmux/vault.json` (modo 600):
  - **Cifra:** a senha passa pelo Argon2id, e o conteúdo é cifrado com XChaCha20-Poly1305.
  - **Na memória:** o daemon guarda os segredos só enquanto o cofre está desbloqueado, e os apaga da memória ao trancar.
  - **Backend trocável:** um keyring do sistema pode entrar depois da v1 sem mudar o resto.
- **Desbloqueio pela TUI e pela linha de comando** (decisão do usuário, 2026-11-14):
  - o daemon sobe no login com o cofre trancado;
  - ao abrir, a TUI mostra um modal pedindo a senha;
  - fora dela, `sandmux vault unlock` pede a senha no terminal.
- **Cadastro de segredos:** cada segredo tem um nome, o valor e, opcionalmente, os domínios donos (para a troca no tráfego dos changes 13 e 25). O valor é lido do terminal sem eco, ou da entrada padrão, nunca de um argumento.
- **Comandos:** `sandmux vault init|unlock|lock|status|add|list|remove|passwd|import`. O `list` mostra só os nomes e os domínios, nunca os valores.
- **Leitura do manifesto da importação:** `sandmux vault import` percorre os manifestos da importação (as chaves que foram trocadas por valores falsos) e pede o valor real de cada uma, que fica no cofre ligado ao valor falso correspondente.
- **`--pass-env` guardado no cofre** (decisão do usuário, 2026-11-14):
  - com o cofre desbloqueado, os valores de `--pass-env` de uma sessão vão cifrados para o cofre;
  - depois de um reboot, a sessão fica "aguardando cofre" e volta sozinha quando o cofre é desbloqueado;
  - com o cofre ausente ou trancado no `run`, nada é guardado: um aviso diz que, num reboot, a sessão fica parada, como hoje.
- **Estado novo de sessão**, "aguardando cofre", no `sandmux ls`, na TUI e na restauração.
- **Protocolo 10:** pedidos do cofre e o estado novo.

## Capabilities

### New Capabilities

- `secret-vault`: arquivo do cofre e cifra, senha, desbloqueio e trancamento, cadastro e listagem de segredos, troca de senha, leitura do manifesto da importação, valores de `--pass-env` guardados por sessão.

### Modified Capabilities

- `session-daemon`:
  - "Ambiente do lançamento vem do cliente" passa a permitir os valores de `--pass-env` no cofre cifrado;
  - "Restauração depois de reboot" ganha o estado "aguardando cofre" para as sessões com valores no cofre.
- `terminal-ui`: entra o requisito "Desbloqueio do cofre na TUI".

## Impact

- **Crate novo `sandmux-vault`:** formato do arquivo, derivação da chave, cifra e o backend de arquivo. Dependências novas: `argon2`, `chacha20poly1305`, `zeroize` e `base64`.
- **`sandmux-proto`:**
  - pedidos `Vault*`;
  - estado `AguardandoCofre`;
  - `PROTOCOL` 10.
- **`sandmux`:**
  - no daemon, o estado do cofre, os segredos na memória, os valores de `--pass-env` por sessão e a restauração "aguardando cofre";
  - os comandos `vault`;
  - o modal de senha na TUI;
  - a leitura dos manifestos da importação.
- **Documentação:** `docs/vault.md` (novo), `docs/sessions.md`, `docs/tui.md` e o README.
- **Migração:** `sandmux daemon restart` (protocolo 10). Sem cofre criado, tudo funciona como hoje.
