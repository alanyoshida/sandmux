# Design

## Context

- **Cofre (change 11):** segredos com domínios donos e, quando vieram da importação, o valor falso; valores só na memória do daemon enquanto desbloqueado.
- **Decifração (change 12):**
  - o daemon já soma os domínios donos à lista de decifração;
  - cada pedido decifrado passa por `mitm::rewrite`, ainda vazio;
  - o gateway roda no supervisor do `sandmux-run`, fora do sandbox, como o mesmo usuário do daemon.
- **Presets:** o opencode já recebe `OPENCODE_CONFIG_CONTENT` apontando a chave do provedor para uma variável quando ela vem do `--pass-env`, porque o `auth.json` importado vence a variável. O hermes lê o `.env` do HOME sintético por cima do ambiente.

## Goals / Non-Goals

**Goals:**

- O harness chama o modelo sem a chave real no sandbox.
- O valor real só sai para o dono do segredo.
- Nada real em disco nem na especificação.

**Non-Goals:**

- Segredos de outras ferramentas (`gh`, registries): change 25.
- OAuth: depois da v1.
- Trocar valores no corpo dos pedidos.

## Decisions

### 1. Valor falso

- **Geração:** `sandmux_vault::new_fake()` gera `sandmux-fake-` mais 32 caracteres hexadecimais aleatórios.
- **Quando:** o `VaultAdd` usa o falso da importação quando vem um; senão, gera. No desbloqueio, segredos sem falso (gravados pelo change 11) ganham um, e o cofre é regravado.
- **`sandmux vault list`:** mostra o valor falso, que é o que o agente vê de qualquer jeito.

### 2. Ambiente falso pelo preset

- **Tabela de provedores:** `presets::PROVIDERS`, com a variável e o domínio de cada provedor.
- **No lançamento de uma sessão do opencode ou do hermes:** com o cofre desbloqueado, o daemon procura cada provedor cujo domínio é dono de algum segredo e passa `(variável, falso)` ao `prepare` (`RunOptions.fake_env`).
- **Na composição:**
  - põe as variáveis no ambiente;
  - no opencode, gera o `OPENCODE_CONFIG_CONTENT` com os nomes do `pass-env` mais os das variáveis falsas;
  - uma variável que também está no `pass-env` sai do `pass-env`, com um aviso: vale a falsa.
- **No hermes:** o `.env` importado tem o falso da importação. A troca funciona quando esse falso está no cofre, depois do `sandmux vault import`; a documentação orienta.
- **No `run` sem terminal:** não há daemon nem cofre, então não há ambiente falso.

### 3. Valores reais pelo socket do daemon

- **Na especificação:** a seção `network` ganha `vault: {socket, data_dir, config_dir}`, preenchida só nas sessões do daemon. São os dados que a saudação do protocolo exige.
- **Na conexão decifrada:**
  - antes de servir, o gateway conecta no socket (numa thread de bloqueio do tokio);
  - faz a saudação e manda `VaultInject { tag }`;
  - a resposta `Injections { secrets: [{name, domains, fake, value}] }` vem só com os segredos que têm domínio, e vazia com o cofre trancado.
- **Na memória:** o gateway filtra os que têm um domínio casando com o SNI e guarda a lista só na memória da conexão.
- **Protocolo 11.**

Alternativas descartadas:

- **Valores na especificação:** o memfd fica com o supervisor por toda a sessão, e trancar o cofre não teria efeito.
- **Um arquivo:** regra do projeto, nada real em disco.

### 4. Troca

`mitm::rewrite(&mut Request, &[Injection]) -> Option<String>`:

- para cada cabeçalho cujo valor contém o falso de um segredo da lista (já filtrada pelo domínio), troca pelo real e devolve o nome do segredo;
- o nome vai para a linha `https` (campo `secret`).

Como a lista vem filtrada pelo SNI, o falso enviado a outro domínio decifrado nunca é trocado.

### 5. Modo só inferência pelo cofre

`NetPolicy::restrict_inference(&mut self, domains)`:

- no modo `inferencia`, se algum item do catálogo de inferência (ligado) é um domínio de segredo, desliga os itens do catálogo que não são;
- o daemon chama isso no `with_vault`, junto da lista de decifração.

## Risks / Trade-offs

- **[Falso conhecido pelo agente]** O agente vê o falso e pode mandá-lo a outro domínio. → Só é trocado no dono; fora dele continua falso e não vale nada.
- **[Cofre trancado com a sessão viva]** Os pedidos saem com o falso e falham com 401. → É o comportamento seguro; o painel mostra o cofre trancado.
- **[Chave no corpo]** Provedores que pedem a chave no corpo não são atendidos. → Os provedores da tabela usam cabeçalho.

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart` (protocolo 11). Os segredos existentes ganham o falso no primeiro desbloqueio. Quem usava `--pass-env OPENROUTER_API_KEY` pode passar a guardar a chave com `sandmux vault add openrouter --domain openrouter.ai`.
