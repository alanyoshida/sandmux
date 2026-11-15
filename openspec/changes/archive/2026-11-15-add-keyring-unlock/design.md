## Context

O cofre é um arquivo cifrado com a senha (Argon2id e XChaCha20-Poly1305). O daemon guarda o cofre aberto na memória, com `PR_SET_DUMPABLE` desligado. O barramento de sessão do D-Bus fica fora do alcance do sandbox.

Nesta máquina, o Secret Service é servido pelo gnome-keyring, ao lado do KWallet. A release é estática, em musl.

## Goals / Non-Goals

**Goals:**
- Abrir o cofre sem digitar a senha, quando o usuário escolher isso.
- Nenhuma consulta ao keyring de quem não ligou.

**Non-Goals:**
- Guardar os segredos no keyring. Só a senha vai para lá.
- Abrir o cofre antes do login (daemon no boot com linger). O keyring do usuário só abre no login; fica para o item "Daemon no boot".
- O keyring do kernel (`keyctl`), que não sobrevive ao reboot.

## Decisions

- **Crate `keyring` 3.x** com `async-secret-service`, `tokio` e `crypto-rust`. É Rust puro: zbus no lugar da `libdbus` em C, que quebraria a release em musl. A senha vai cifrada pela sessão do Secret Service. O tokio já é dependência do projeto.
  - Alternativa: o `secret-tool` da libsecret. Recusada: nem toda máquina o tem, inclusive esta.
- **O item no keyring:** serviço `sandmux` e usuário igual à pasta de dados (`~/.local/share/sandmux`). Assim, duas instalações (ou os testes) nunca se misturam.
- **Marcador de ligado:** `vault.keyring` na pasta de dados, sem nada secreto dentro. O daemon só fala com o keyring quando ele existe, e esse é o "opcional" da decisão: quem não ligou nunca vê uma consulta nem um pedido de desbloqueio do keyring.
- **O daemon faz tudo**, inclusive gravar no `on`. O `KeyringOn` leva a senha, e o daemon a confere decifrando o arquivo de novo, sem mexer no cofre aberto. Assim, o que o `vault keyring` mostra é o que o daemon alcança, com o mesmo barramento que ele usa para abrir o cofre.
- **Quando tentar:**
  - ao subir, antes de atender os clientes, depois da restauração das sessões. Assim o primeiro cliente já encontra o cofre aberto, e a subida espera no máximo os 3 segundos do limite. A trava fica com a tentativa inteira, e um pedido de estado que chega durante outra tentativa espera por ela;
  - no `VaultStatus` com o cofre trancado, no máximo a cada 5 segundos, esperando até 3 segundos pela resposta para não segurar a conexão.

  A TUI pergunta o estado antes de mostrar o pedido de senha, então o cofre abre antes do modal aparecer.
- **O efeito do desbloqueio** é o do `VaultUnlock`:
  - as sessões que aguardam o cofre sobem;
  - as regras de rede são recompiladas;
  - o vigia de chaves acorda.

  Uma função só, usada pelos dois caminhos.
- **`vault passwd`:** depois de o daemon trocar a senha, ele regrava o item, se o marcador existe. Se o keyring falhar, a troca vale, e o aviso diz para rodar `vault keyring on` de novo.
- **Protocolo 28:** `KeyringStatus` → `KeyringInfo { enabled, available, error }`; `KeyringOn { password }` → `Ok` ou erro; `KeyringOff` → `Ok`.

## Risks / Trade-offs

- [Com o keyring ligado, qualquer processo do usuário fora do sandbox lê a senha enquanto o keyring da sessão está aberto] → É a troca aceita ao ligar, e a documentação a explica. O sandbox não alcança o D-Bus. Desligado, que é o padrão, nada muda.
- [Keyring trancado pede a senha dele num diálogo] → Só acontece com o keyring ligado, e o diálogo é do próprio keyring do sistema.
- [Mais 130 crates de dependência] → Todos em Rust; o binário cresce. Aceito, por ser o caminho que funciona na release estática.
