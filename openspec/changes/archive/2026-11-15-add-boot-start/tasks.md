## 1. Daemon

- [x] 1.1 `SessionState::AguardandoLogin` (texto, marca na TUI), protocolo 31; leitura do estado do login pelo logind (`SANDMUX_LOGIND_USER`)
- [x] 1.2 Restauração adiada sem login, a thread que espera o login (relança, tenta o keyring, avisa o cofre); keyring e aviso do cofre na subida só com login

## 2. CLI

- [x] 2.1 `sandmux daemon boot [on|off]` (`SANDMUX_LOGINCTL`, `SANDMUX_LINGER_DIR`), a linha no `daemon status` e no `doctor`

## 3. Testes e documentação

- [x] 3.1 Testes com o logind e o `loginctl` falsos: subida sem login (abas aguardando, nenhum harness), login relança, reinício antes do login mantém a espera, subida com login restaura como antes, `boot on`/`off`/estado, `loginctl` que recusa
- [x] 3.2 Documentação: `docs/sessions.md`, `docs/install.md` e os pares em inglês
