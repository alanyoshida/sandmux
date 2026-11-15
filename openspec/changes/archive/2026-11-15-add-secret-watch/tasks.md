## 1. Base

- [x] 1.1 Inferência de domínio na biblioteca (`sandmux::secretdomains`), com a CLI e o doctor usando a mesma; lista dos harnesses varridos e pastas a observar em `sandmux-import`
- [x] 1.2 `SessionInfo.keys` (movidas, esperando o cofre, esperando domínio), protocolo 27

## 2. Vigia no daemon

- [x] 2.1 Thread do vigia: inotify nas pastas, espera de 500 ms, varredura ao subir, a cada 60 segundos (`SANDMUX_SECRET_SCAN_SECS`) e ao abrir o cofre; movimento arquivo a arquivo com a conferência antes de regravar; registro sem valores
- [x] 2.2 Contadores nas sessões: movidas desde o lançamento (zeradas no lançamento) e as esperando, preenchidas no `list`

## 3. Avisos, testes e documentação

- [x] 3.1 TUI: marca da aba, aviso na barra e linha "chaves reais" no painel; `sandmux doctor` diz que o daemon move sozinho
- [x] 3.2 Testes: movimento por gravação com o cofre aberto, varredura periódica, espera com o cofre trancado e movimento ao abrir, domínio desconhecido, nenhum valor real no registro
- [x] 3.3 Documentação: `docs/vault.md`, protocolo em `docs/sessions.md` e os pares em inglês
