# Design

## Context

- **Daemon (change 2):** guarda as sessões na memória e numa lista em `$XDG_DATA_HOME/sandmux/sessions.json`, sem valores de variáveis. Os valores de `--pass-env` chegam do cliente no `Create` e no `Restart` e só vivem na memória; na restauração, uma sessão com `pass-env` fica parada.
- **Importação (change 1):** troca cada segredo dos arquivos de um harness por um valor falso com o prefixo `sandmux-fake-` e grava um manifesto por harness em `<dados>/manifests/<harness>.json`, com o arquivo, a chave e o valor falso (nunca o real).
- **Socket do daemon:** privado do usuário (pasta 700, socket 600, `SO_PEERCRED`).

## Goals / Non-Goals

**Goals:**

- Nenhum segredo nem nome de segredo em claro no disco.
- O daemon sobe sozinho no login e espera a senha sem bloquear as sessões que não dependem do cofre.
- Base para a troca de chave no tráfego (changes 12, 13 e 25): segredos com domínios donos e com o valor falso correspondente.

**Non-Goals:**

- Trocar chaves no tráfego (changes 12, 13 e 25).
- Keyring do sistema (depois da v1); o backend fica trocável.
- Desbloqueio sem senha ou por biometria.

## Decisions

### 1. Crate `sandmux-vault`, com RustCrypto

- **Formato do arquivo**, JSON, gravado de forma atômica com modo 600:

  ```json
  {"version":1,"kdf":{"alg":"argon2id","m_kib":65536,"t":3,"p":1,"salt":"<base64>"},
   "nonce":"<base64 24 bytes>","data":"<base64>"}
  ```

- **Conteúdo cifrado** (JSON):
  - `secrets`: `{name, value, domains, fake}`;
  - `pass_env`: mapa de sessão para `{variável: valor}`.
- **Chave:** Argon2id (64 MiB, 3 passadas, 1 via) sobre a senha, com sal aleatório de 16 bytes, dá 32 bytes.
- **Cifra:** XChaCha20-Poly1305 com nonce aleatório de 24 bytes, novo a cada gravação. O cabeçalho entra como dado associado, para não ser trocado.
- **Memória:**
  - enquanto desbloqueado, o daemon guarda a chave derivada e o conteúdo (em tipos com `zeroize`), para regravar sem pedir a senha de novo;
  - trancar apaga os dois.
- **Backend:** `trait Store { fn load(&self) -> io::Result<Option<Vec<u8>>>; fn save(&self, bytes: &[u8]) -> io::Result<()> }`, com `FileStore`; um keyring entra depois como outro `Store`.

Alternativa descartada: `age` com senha. Resolveria a cifra, mas trancaria o formato no scrypt do `age` e não dá a chave derivada para regravar sem a senha.

### 2. Pedidos do protocolo 10

| Pedido | Resposta | Efeito |
|---|---|---|
| `VaultStatus` | `VaultInfo { exists, unlocked, secrets }` | |
| `VaultInit { password }` | `Ok` | cria o cofre já desbloqueado; falha se já existe |
| `VaultUnlock { password }` | `Ok` | ou `Error` com "senha não confere" |
| `VaultLock` | `Ok` | |
| `VaultPasswd { old, new }` | `Ok` | |
| `VaultAdd { name, value, domains, fake }` | `Ok` | substitui um segredo de mesmo nome |
| `VaultRemove { name }` | `Ok` | |
| `VaultList` | `VaultEntries { entries: [{name, domains, fake}] }` | sem valores |

- **Senha:** vai em claro pelo socket privado do usuário, como os valores de `--pass-env` já vão.
- **Estado novo de sessão:** `SessionState::AguardandoCofre`, mostrado como "aguardando cofre".

### 3. `--pass-env` no cofre

- **No `Create` e no `Restart` com valores de `pass-env`:**
  - com o cofre desbloqueado, o daemon grava `pass_env[<sessão>]` no cofre e marca `vault_env = true` na lista de sessões;
  - sem cofre, ou com ele trancado, devolve um `Warning` dizendo que, num reboot, a sessão fica parada.
- **Restauração:** uma sessão viva com `pass-env` e `vault_env` volta como `AguardandoCofre`; sem `vault_env`, parada como antes.
- **Desbloqueio:** o daemon relança cada sessão `AguardandoCofre` com as variáveis padrão e os valores do cofre. Uma sessão cujos valores sumiram do cofre fica parada com o motivo.
- **Limpeza:**
  - `kill` tira os valores da sessão do cofre (com ele desbloqueado);
  - no desbloqueio, o daemon também tira os de sessões que não existem mais.
- **Lista de sessões:** guarda só o marcador `vault_env`, sem nada do cofre.

### 4. Leitura do manifesto (`sandmux vault import`)

O cliente:

- lê os manifestos de `<dados>/manifests/`;
- pede ao daemon a lista do cofre;
- para cada valor falso ainda sem segredo ligado, pede o valor real, sem eco;
- manda `VaultAdd` com:
  - o nome `<harness>:<arquivo>:<chave>`;
  - o valor falso;
  - os domínios conhecidos pelo nome da chave (por exemplo, `OPENROUTER_API_KEY` e a chave `openrouter` do `auth.json` levam a `openrouter.ai`); sem nenhum conhecido, o segredo fica sem domínio.

Uma resposta vazia pula o item.

### 5. Senha no terminal e na TUI

- **No terminal:** a leitura desliga o eco por `termios` e restaura o terminal no fim, mesmo com Ctrl+C. Sem terminal, lê a primeira linha da entrada padrão, para scripts e testes.
- **Na TUI:** ao abrir, ela pede `VaultStatus` e, com o cofre existente e trancado, abre o modal `Unlock`, que mostra um `•` por caractere. Enter manda `VaultUnlock`; um erro fica no modal; Esc fecha.

## Risks / Trade-offs

- **[Senha esquecida]** Sem a senha, o cofre não abre. → `sandmux vault init --force` recria o cofre vazio, depois de confirmar.
- **[Daemon na memória]** Um processo do mesmo usuário fora do sandbox pode ler a memória do daemon. → Fora da ameaça alvo, que é o agente dentro do sandbox. O daemon marca o próprio processo como não despejável (`PR_SET_DUMPABLE` desligado) enquanto o cofre está aberto.
- **[Custo do Argon2id]** Cerca de 0,1 a 0,5 s por desbloqueio. → Só no desbloqueio e na troca de senha; as gravações usam a chave já derivada.

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart` (protocolo 10). Sem `sandmux vault init`, nada muda: as sessões com `--pass-env` continuam ficando paradas num reboot. A lista de sessões ganha `vault_env` com valor padrão falso.
