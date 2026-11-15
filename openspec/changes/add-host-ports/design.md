## Context

O gateway de cada sandbox roda no supervisor (`sandmux-run`), fora do namespace de rede do sandbox. A publicação de portas de entrada (change 10) já usa um auxiliar que entra nos namespaces de usuário e de rede do sandbox e abre conexões no `127.0.0.1` de lá, devolvendo o descritor por `SCM_RIGHTS` (`sandmux-launcher/src/connector.rs`). O `127.0.0.1` do sandbox é dele; o host é `host.sandmux.internal` (`10.0.2.254`), levado ao `127.0.0.1` do host pelo gateway.

## Goals / Non-Goals

**Goals:**
- O `127.0.0.1:porta` de dentro chega ao do host para as portas escolhidas, sem editar a configuração do harness.
- Registro, bloqueio por regra e mudança ao vivo, como no resto da rede.

**Non-Goals:**
- Reescrever URLs da configuração importada. O usuário escolheu encaminhar as portas.
- UDP e IPv6 (`[::1]`). Servidores de modelo e MCPs locais usam TCP no `127.0.0.1`.
- Encaminhar portas no modo sem rede. Lá não há gateway.

## Decisions

### O auxiliar abre ouvintes

O protocolo do auxiliar passa de 2 bytes (a porta) para 3: a operação e a porta.

- **`C`:** conectar, como antes.
- **`L`:** escutar em `127.0.0.1:porta` dentro do sandbox. A resposta é o descritor do ouvinte, com a marca `L`, ou `E`.

O ouvinte é criado no namespace de rede do sandbox, e o supervisor o usa direto: um socket guarda o namespace em que nasceu.

### `hostports::run`

É um laço como o `ports::run`, a cada segundo. Ele relê `NetPolicy.host_ports` e compara com os ouvintes abertos:

- **porta nova:** pede o ouvinte ao auxiliar e sobe uma tarefa que aceita conexões;
- **porta tirada:** aborta a tarefa, e o ouvinte fecha.

Para cada conexão:
1. confere a regra de bloqueio para `host.sandmux.internal` e para o IP `127.0.0.1`;
2. conecta no `127.0.0.1:porta` do host;
3. copia nos dois sentidos;
4. escreve a linha `tcp` no registro, com o bloqueio quando houver.

Uma porta já ocupada dentro do sandbox (o agente escuta nela) não abre. O registro ganha uma linha com o motivo, e o laço tenta de novo a cada segundo.

A porta do host é uma permissão explícita da configuração, então vale em qualquer modo com rede, inclusive `custom` e `inferencia`. Só a denylist a recusa.

### Sem eco na publicação

O `ports::run` lê as portas em escuta do sandbox pelo `/proc/<pid>/net/tcp`, e os ouvintes do encaminhamento aparecem lá. O `Live` das portas de entrada ganha a lista das portas do host, que o `desired` tira.

### Configuração

- **Chave:** `host-ports`, uma lista de números de 1 a 65535 em qualquer camada (`HOST_PORTS_KEY`).
- **Linha de comando:** `--host-port` (repetível) no `run` e `LaunchOptions.host_ports` (`serde default`).
- **Edição:** `sandmux config add|remove host-ports`.
- **Política efetiva:** soma sem repetição; vai em `NetPolicy.host_ports` (`serde default`).
- **Aba de chat:** `confine_chat` limpa a lista.

### Aviso da importação

O `sandmux-import` procura, nos arquivos de configuração que copiou do harness (`opencode.json`, `opencode.jsonc` e o `config.yaml` do hermes), `127.0.0.1:N`, `localhost:N` e `[::1]:N`. O relatório da importação ganha um aviso por porta, com o comando `sandmux config add host-ports N --global`.

### Protocolo 22

`LaunchOptions.host_ports`.

## Risks / Trade-offs

- **[Serviços do host ao alcance do agente]** A porta é escolhida pelo usuário, e cada conexão vai para o registro. Um serviço sem senha numa porta liberada fica ao alcance do agente, como já acontece com `host.sandmux.internal` no modo aberta.
- **[Porta ocupada dentro do sandbox]** Se o agente já escuta na porta, o encaminhamento não abre e o registro diz o motivo.
