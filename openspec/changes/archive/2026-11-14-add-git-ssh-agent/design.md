# Design

## Context

- **Cofre (change 11):** segredos cifrados com Argon2id e XChaCha20-Poly1305; o daemon guarda o conteúdo na memória enquanto desbloqueado e fica não despejável.
- **Troca no tráfego (changes 13 e 25):** os segredos de token têm valor falso e domínios donos; o gateway pede os valores ao daemon pelo socket privado.
- **Docker privado (change 24):** o daemon já entrega ao sandbox um socket por sessão, montado por bind num caminho fixo.
- **Gateway (changes 8 e 9):** lê os primeiros bytes do cliente (até 300 ms) para o SNI e o `Host`, decide pela política do arquivo do lançamento e registra cada conexão em JSONL.
- **HOME sintético:** guarda a configuração das ferramentas entre execuções; o `~/.ssh` do host não entra no sandbox.

## Goals / Non-Goals

**Goals:**

- `git clone`, `git fetch` e `git push` por SSH, e `ssh -T git@github.com`, sem a chave privada no sandbox.
- Cada assinatura registrada; SSH de saída só aos servidores das chaves escolhidas.

**Non-Goals:**

- Repassar o ssh-agent do host, chaves em hardware (YubiKey) e agentes de terceiros (decisão do usuário: chaves no cofre).
- Certificados OpenSSH, confirmação por assinatura e restrição de destino pelo `session-bind@openssh.com`.
- Chaves SSH dentro dos contêineres do Docker privado.

## Decisions

### 1. Chave SSH no cofre

- **Segredo:** `Secret` ganha `kind` (`token`, padrão, ou `ssh`). Numa chave SSH, o valor é a chave privada OpenSSH sem passphrase, e os domínios são os servidores. Não há valor falso nem ferramentas.
- **Fora da troca:** os segredos `ssh` ficam fora das injeções, das chaves falsas, das ferramentas e da lista de decifração do TLS.
- **Cadastro:** `sandmux vault add-ssh <nome> --file <chave> --server <domínio>...`.
  - O cliente lê o arquivo (o usuário tem acesso a ele; o daemon não precisa) e, se a chave é cifrada, pede a passphrase sem eco e decifra com o crate `ssh-key`.
  - Manda ao daemon a chave sem passphrase pelo socket privado (`VaultAddSsh`), como o valor de um token.
  - Recusa chaves de tipos fora de Ed25519, ECDSA (P-256, P-384) e RSA, e exige ao menos um `--server` exato.
- **Lista:** `VaultEntry` ganha `kind` e `fingerprint`; o `vault list` mostra `ssh  (github.com)  SHA256:...`.
- **Protocolo:** 13.

### 2. Chaves escolhidas por perfil

- **Configuração:** chave `ssh-keys` (lista de nomes, acumulada entre camadas como as outras listas; `"*"` é todas as chaves SSH do cofre) e `--ssh-key <nome>` (repetível) no `run`.
- **Sem nenhuma:** não há socket nem `SSH_AUTH_SOCK`.
- **Nome desconhecido:** com o cofre desbloqueado, o lançamento avisa; com ele trancado, nada é conferido.

### 3. Agente por lançamento

- **Socket:** o daemon cria `$XDG_RUNTIME_DIR/sandmux/ssh/<etiqueta>.sock` (pasta 700, socket 600) e uma thread que aceita conexões. A composição monta o socket por bind em `/run/sandmux/ssh-agent.sock` e define `SSH_AUTH_SOCK`. O socket some com o lançamento.
- **Protocolo do agente** (subconjunto do `draft-miller-ssh-agent`):
  - `REQUEST_IDENTITIES` (11) responde com as chaves escolhidas que estão no cofre desbloqueado;
  - `SIGN_REQUEST` (13) assina com a chave pedida, se ela está entre as escolhidas. Em RSA, as flags `rsa-sha2-256` (2) e `rsa-sha2-512` (4) escolhem o hash, e o `ssh-rsa` com SHA-1 é recusado;
  - qualquer outro pedido (adicionar, remover, trancar, extensões como `session-bind@openssh.com`) recebe `FAILURE` (5).
- **Cofre trancado:** a lista vem vazia, e a assinatura falha.
- **Resolução a cada pedido:** as chaves são lidas do cofre a cada pedido; um desbloqueio depois da subida passa a valer na hora, e uma chave removida deixa de valer.

### 4. Registro

Cada pedido de assinatura vira uma linha no JSONL do dia, gravada pelo daemon com `O_APPEND` (linha menor que `PIPE_BUF`):

- `proto: "ssh-sign"`, a etiqueta, `secret` com o nome da chave, `fingerprint` e `verdict` (`allow` ou `deny`, com o motivo `cofre trancado` ou `chave não escolhida`).
- O `sandmux net` e o painel mostram `assinatura ssh: <nome>`.

### 5. SSH só para os servidores das chaves

- **Política:** `NetPolicy` ganha `ssh: Vec<Pattern>`, os servidores das chaves escolhidas, somados pelo daemon a cada compilação (como os domínios do cofre no `with_vault`). Com o cofre trancado, a lista fica vazia.
- **Reconhecimento:** uma conexão TCP é SSH quando os primeiros bytes do cliente começam com `SSH-`, ou, num cliente que espera o servidor falar primeiro, quando os primeiros bytes do servidor começam com `SSH-`.
- **Decisão:** em qualquer porta e em qualquer modo, uma conexão SSH só segue quando um nome ligado a ela casa com um padrão de `ssh`; senão é fechada e registrada como bloqueada com o motivo `ssh fora dos servidores das chaves`. A decisão do modo vale antes: no `custom`, o `github.com` precisa estar liberado.

### 6. `known_hosts` da sessão

- `Mount::File` em `~/.ssh/known_hosts`: o guardado no HOME sintético mais as linhas do `~/.ssh/known_hosts` do host cujos nomes casam com os servidores das chaves escolhidas.
- **Nomes em hash** (`|1|sal|hash`): casam quando o HMAC-SHA1 do nome com o sal é o hash.
- **Nomes com porta** (`[host]:porta`) e listas separadas por vírgula também casam; as linhas `@cert-authority` e `@revoked` que casam também entram.
- O que o `ssh` grava no `known_hosts` dentro do sandbox fica só naquela execução.
- **`/etc/ssh/ssh_config.d`** (achado da verificação): o `ssh` recusa um arquivo incluído cujo dono não é o root nem o usuário, e no user namespace o root do host aparece como `nobody`. Numa sessão com agente, a pasta vira um tmpfs com cópias dos arquivos `.conf` (links seguidos), do usuário do sandbox. Inclusões aninhadas fora dessa pasta não são cobertas.

## Risks / Trade-offs

- **[Assinatura livre]** Enquanto a aba vive e o cofre está aberto, o agente assina quantas vezes quiser (risco já aceito no roadmap). → Cada assinatura fica no registro, e o SSH só chega aos servidores das chaves.
- **[Chave sem passphrase na memória do daemon]** A chave decifrada fica na memória do daemon enquanto o cofre está aberto. → É o mesmo tratamento dos tokens; o daemon é não despejável.
- **[Servidor que fala primeiro]** Um cliente que espera o servidor manda nada nos primeiros 300 ms. → O gateway olha também os primeiros bytes do servidor antes de repassá-los.
- **[known_hosts ausente no host]** Sem linhas no host, o `ssh` pede a confirmação da chave do servidor na primeira conexão. → A documentação orienta conectar uma vez pelo host, ou aceitar dentro da sessão.

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart` (protocolo 13). Nada muda para quem não cadastra chaves SSH.
