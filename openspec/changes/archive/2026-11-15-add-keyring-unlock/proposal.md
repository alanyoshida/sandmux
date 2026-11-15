## Why

Item "Keyring" da lista depois da v1, escolhido pelo usuário (2026-11-15).

Hoje o daemon sobe com o cofre trancado, e a senha é digitada a cada subida: na TUI, ou com `sandmux vault unlock`. Depois de um reboot, as abas que usam o cofre esperam essa senha.

Decisões do usuário (2026-11-15):

- **O que vai para o keyring:** só a senha do cofre. O arquivo cifrado continua como está.
- **Ativação:** opcional, por comando. O keyring fica desligado até o usuário pedir.

## What Changes

- **`sandmux vault keyring on`:** pede a senha do cofre, confere com o arquivo e a guarda no keyring do sistema, pelo Secret Service (`org.freedesktop.secrets`: gnome-keyring, KWallet, KeePassXC).
- **`sandmux vault keyring off`:** apaga a senha do keyring.
- **`sandmux vault keyring`:** diz se está ligado e se o keyring responde.
- **Desbloqueio sozinho:** com o keyring ligado, o daemon abre o cofre com a senha do keyring:
  - ao subir;
  - quando um cliente pergunta o estado do cofre e ele está trancado (como a TUI faz ao abrir), no máximo a cada 5 segundos.

  O efeito é o de um `vault unlock`: as abas que esperavam o cofre sobem, e o vigia de chaves acorda. Sem o keyring, ou com uma senha que não abre mais o cofre, nada muda: o cofre fica trancado e a senha é pedida como hoje.
- **`sandmux vault passwd`:** com o keyring ligado, atualiza a senha guardada nele.
- **TUI:** a seção Cofre das configurações ganha o item "Keyring". Ele liga, pedindo a senha, e desliga.
- **`sandmux doctor`:** diz se o keyring está ligado.
- **Protocolo 28:** as mensagens do keyring.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `secret-vault`: a senha do cofre no keyring do sistema e o desbloqueio por ele.

## Impact

- **Dependência nova:** o crate `keyring` (3.x), com o Secret Service em Rust puro (zbus, tokio e criptografia em Rust). Compila para o musl estático da release, sem a `libdbus` em C.
- **`sandmux` (lib):**
  - módulo do keyring (ler, guardar e apagar, com o marcador de ligado);
  - daemon: as mensagens novas, o desbloqueio ao subir e no estado, e a troca de senha;
  - CLI: `vault keyring`;
  - TUI: o item nas configurações;
  - doctor.
- **`sandmux-proto`:** `KeyringStatus`, `KeyringInfo`, `KeyringOn` e `KeyringOff`; protocolo 28.
- **Testes:** com um barramento de sessão e um gnome-keyring próprios do teste (`dbus-run-session`), nunca o keyring do usuário. O teste é pulado onde essas ferramentas não existem.
- **Documentação:** `docs/vault.md`, README, `docs/tui.md` e os pares em inglês.
