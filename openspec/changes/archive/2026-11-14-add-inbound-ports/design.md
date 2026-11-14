# Design

## Context

- **Change 10:**
  - o supervisor vigia `/proc/<pid>/net/tcp` a cada segundo;
  - para cada porta em escuta, abre um ouvinte no `127.0.0.1` do host (o mesmo número ou o próximo livre);
  - leva cada conexão ao sandbox pelo auxiliar de conexão;
  - a lista vai para um arquivo que o daemon lê.
- **Política de rede:** o gateway relê o arquivo da política (`Policy::get`), que o daemon regrava quando a configuração ou a aba mudam.

## Goals / Non-Goals

**Goals:**

- Escolher quais portas saem do sandbox e em que endereço, com registro das conexões, mudando ao vivo.

**Non-Goals:**

- IPv6 e UDP de entrada.
- Filtro por origem nas portas de entrada (quem pode conectar).
- TLS na porta publicada (o servidor do agente é que fala TLS, se quiser).

## Decisions

### 1. Regras

- **Tipo:** `PortRule { port: u16, addr: Option<Ipv4Addr> }` em `sandmux-spec::net`.
- **Na política:** `NetPolicy.ports: Vec<PortRule>` (padrão vazio, `serde(default)`).
- **Texto:** `8642` (só `127.0.0.1`) ou `8642@0.0.0.0` / `8642@192.168.1.10`.
  - O endereço precisa ser IPv4;
  - `127.0.0.1` no endereço equivale a não dar endereço.
- **Chave e opção:** `ports` (lista, somada entre as camadas) e `--port` no `run` (repetível).
- **Aviso:** quando um endereço diferente de `127.0.0.1` aparece, o lançamento avisa que a porta fica exposta à rede.

### 2. Publicação

A cada segundo, o vigia lê a política atual (`Policy::get`) e as portas em escuta, e reconcilia:

- **Sem regras:** toda porta no `127.0.0.1`, como antes.
- **Com regras:** só as portas com regra. Cada uma recebe:
  - o ouvinte no `127.0.0.1` (mesmo número ou o próximo livre);
  - com endereço, um segundo ouvinte em `endereço:mesmo número`. Se o `bind` falhar, ele não existe, e a falha vai para o registro uma vez (linha `inbound` com `blocked`).
  - com `0.0.0.0`, um ouvinte só, em `0.0.0.0:mesmo número`, que cobre também o `127.0.0.1` (o Linux recusa o `0.0.0.0:p` com o `127.0.0.1:p` já em escuta). Se ele falhar, a porta fica só no `127.0.0.1` (o próximo livre), e a falha vai para o registro.
- **Mudança de regra:** uma porta que saiu da allowlist, ou cujo endereço mudou, tem os ouvintes fechados e, se for o caso, reabertos.

### 3. Registro

- **Quando:** com regras, cada conexão aceita vira uma linha `inbound` no registro de rede, gravada quando ela termina.
- **Campos:** `domain` (o IP de quem conectou), `ip` (o mesmo), `port` (a do sandbox), os bytes nos dois sentidos e a duração.
- **Sem regras:** nada é registrado, como antes, para não encher o registro com o servidor de desenvolvimento.

### 4. Lista

- **Arquivo e protocolo:** `Published` e `PortInfo` ganham `addr` (opcional).
- **Exibição:** o `stats` e o painel mostram `8642 → 127.0.0.1:8642` e, com um endereço específico, mais uma linha `8642 → 192.168.1.10:8642`. Com `0.0.0.0`, o ouvinte único aparece numa linha só, `8642 → 0.0.0.0:8642`.

## Risks / Trade-offs

- **[Exposição à rede]** Uma porta em `0.0.0.0` aceita conexões de qualquer máquina que alcance o host. → Só com regra explícita, com aviso no lançamento e cada conexão no registro.
- **[Número ocupado no endereço]** Não há número alternativo fora do `127.0.0.1`, porque um webhook precisa do número combinado. → Falha registrada.

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart` (o protocolo ganha campos opcionais; vai para 18). Sem `ports`, nada muda.
