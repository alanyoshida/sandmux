## 1. Keyring

- [x] 1.1 Dependência `keyring` (Secret Service em Rust puro) e módulo de keyring: ler, guardar e apagar a senha pelo serviço `sandmux` e a pasta de dados; marcador `vault.keyring`
- [x] 1.2 Protocolo 28: `KeyringStatus`, `KeyringInfo`, `KeyringOn`, `KeyringOff`

## 2. Daemon e CLI

- [x] 2.1 Daemon: as mensagens novas (com a conferência da senha sem mexer no cofre aberto); o desbloqueio ao subir e no `VaultStatus` (a cada 5 s no máximo, espera de 3 s), com o mesmo efeito do `VaultUnlock`; a regravação no `passwd`; registro sem a senha
- [x] 2.2 CLI: `sandmux vault keyring [on|off]`; linha do keyring no `sandmux doctor`

## 3. TUI, testes e documentação

- [x] 3.1 TUI: o item "Keyring" na seção Cofre das configurações (liga com a senha, desliga)
- [x] 3.2 Testes com barramento e gnome-keyring próprios (`dbus-run-session`), pulados sem as ferramentas: ligar, senha errada, desbloqueio depois de reiniciar o daemon, desligado sem consulta, `passwd`
- [x] 3.3 Documentação: `docs/vault.md`, README, `docs/tui.md`, protocolo em `docs/sessions.md` e os pares em inglês
