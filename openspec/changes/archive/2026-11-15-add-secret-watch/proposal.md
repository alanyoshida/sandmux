## Why

Pedido do usuário (2026-11-15): o doctor deve achar sozinho as chaves reais gravadas dentro do sandbox, levá-las ao cofre e deixar um falso no lugar, sem esperar alguém rodar `sandmux doctor --fix`.

Hoje, quando o usuário conecta um provedor pelo próprio harness (o "Connect provider" do opencode, o `hermes setup`), a chave real fica num arquivo do HOME sintético até o `doctor --fix`. Nesse tempo, o agente e as ferramentas da aba podem lê-la.

Decisões do usuário (2026-11-15):

- **Gatilho:** os dois. Um aviso do kernel quando o arquivo muda (inotify), e uma varredura periódica como rede de segurança.
- **Quando não dá para mover sozinho** (cofre trancado ou ausente, domínio desconhecido): avisar e mover depois. O arquivo fica como está até o cofre abrir, ou até o `doctor --fix`.

## What Changes

- **Vigia de chaves no daemon:** o `sandmuxd` observa os arquivos de credencial dos HOMEs sintéticos do opencode e do hermes, os mesmos que o `doctor` lê:
  - quando o harness termina de gravar um deles, o daemon varre o arquivo;
  - o daemon também varre tudo ao subir, a cada 60 segundos e logo depois de o cofre ser aberto.
- **Movimento automático:** a chave real vai ao cofre com um falso novo e o domínio do provedor, como no `doctor --fix`, e o arquivo é regravado com o falso. Arquivo a arquivo: se uma chave do arquivo não pode ir, o arquivo fica como estava. O arquivo também fica como estava se o harness o regravou durante a varredura.
- **Avisos:**
  - **Na aba:** a lista de sessões passa a dizer, por aba, quantas chaves foram movidas desde o lançamento (o harness ainda tem a real na memória, então a aba precisa ser reiniciada) e quantas esperam o cofre ou um domínio.
  - **Na TUI:** a aba ganha uma marca, a barra ganha um aviso e o painel ganha a linha "chaves reais".
- **`sandmux doctor`** diz que o daemon faz esse movimento sozinho, e o `--fix` continua resolvendo o que ficou esperando um domínio.
- **Protocolo 27:** o campo novo na lista de sessões.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `secret-vault`: o movimento automático das chaves reais dos HOMEs sintéticos para o cofre.

## Impact

- **`sandmux` (lib):**
  - a inferência de domínio (`provider_of`, `infer_domains_for` e afins) passa da CLI para a biblioteca, para o daemon usar;
  - módulo novo do vigia no daemon (thread com inotify e varredura periódica);
  - o estado das chaves esperando no `Shared`;
  - o aviso à vigia ao abrir o cofre.
- **`sandmux-import`:** as pastas a observar de cada harness e a lista dos harnesses varridos.
- **`sandmux-proto`:** o campo das chaves em `SessionInfo`; protocolo 27.
- **TUI:** a marca da aba, o aviso na barra e a linha no painel.
- **Testes e documentação:** `docs/vault.md` e o par em inglês; o protocolo em `docs/sessions.md` e no par.
