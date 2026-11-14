# Tetos de memória e de processos

Cada sandbox roda com um teto de memória e um de processos. Um agente descontrolado, seja uma compilação que não para, um teste em loop ou uma fork bomb vinda de prompt injection, esbarra no teto da própria sessão sem derrubar a máquina nem as outras sessões.

## Padrões

| Teto | Padrão | Ao estourar |
|---|---|---|
| Memória | 75% da RAM da máquina, calculada a cada lançamento. O sandbox não usa swap | O kernel mata processos só daquele sandbox, a começar pelo que mais usa memória. O resto da sessão continua |
| Processos e threads | 8192, somados | Criar processo falha só dentro do sandbox (`fork: Resource temporarily unavailable`) |

Os tetos valem para as sessões do daemon e para o `run` sem terminal.

## Ajustar

As chaves `memory` e `processes` do [arquivo de configuração](config.md), e as opções `--memory` e `--processes` do `run`, seguem as mesmas camadas das outras chaves. Vence a camada mais específica, e a opção do `run` vence todas.

| Chave e opção | Valores |
|---|---|
| `memory`, `--memory` | Porcentagem da RAM, de 1% a 100% (`"50%"`); tamanho com `K`, `M`, `G` ou `T` (`"16G"`, `"512M"`); ou `"max"`, sem teto. Mínimo: `128M` |
| `processes`, `--processes` | Um número (`512`) ou `"max"`, sem teto. Mínimo: 32 |

```toml
[global]
memory = "50%"

[project."/home/ana/monorepo"]
memory = "max"
processes = 16384
```

```bash
sandmux config set memory 16G --profile pesado
```

```bash
sandmux run shell --processes 256
```

## Como funciona

Cada lançamento roda num escopo transitório do systemd de usuário, criado com `systemd-run --user --scope`, com o lançador dentro dele. As propriedades do escopo são:

- `MemoryMax`, o teto de memória;
- `MemorySwapMax=0`;
- `TasksMax`, o teto de processos;
- `OOMPolicy=continue`, para o systemd não parar a sessão inteira na primeira morte por falta de memória.

O escopo some quando o sandbox termina.

O agente não consegue mexer nos tetos. Dentro do sandbox, `/sys/fs/cgroup` é somente leitura, então gravar em `memory.max`, em `pids.max` ou em `cgroup.procs`, ou criar um cgroup, é negado. A suíte de fuga tenta tudo isso.

### Sem suporte

Os tetos precisam de duas coisas:

- o systemd de usuário respondendo ao `systemd-run --user`;
- os controladores `memory` e `pids` delegados ao `user@<uid>.service`.

Sem isso, a sessão roda sem teto, e cada lançamento avisa:

```
sandmux: aviso: esta sessão roda sem teto de memória e de processos: <motivo>; veja `sandmux doctor`
```

O `sandmux doctor` mostra a causa. O sandbox funciona mesmo assim: o teto protege a máquina, mas não é uma fronteira de segurança.

## Consumo

`sandmux ls` mostra, para cada sessão viva, a memória e os processos em uso, cada um com o teto:

```
ID  NOME           ESTADO                             CLIENTES  MEMÓRIA     PROC     PASTA
1   opencode:proj  viva                               1         412M/34,5G  37/8192  /home/ana/proj
2   shell:proj     encerrada (137, memória esgotada)  0                              /home/ana/proj
```

`sandmux stats <id>` mostra o detalhe:

- memória em uso, pico e teto;
- processos em uso, pico e teto;
- tempo de CPU usado;
- processos mortos por falta de memória;
- criações de processo recusadas pelo teto.

Uma sessão que terminou depois de uma morte por falta de memória aparece como "memória esgotada".

O `sandmux stats` termina com o resumo da proteção do último lançamento: nível, camada de escrita, pastas, GPU e KVM, clipboard, `pass-env`, tetos e rede. É o mesmo resumo do painel lateral da TUI.

O Docker privado de cada pasta tem os próprios tetos, com os valores da pasta, e aparece numa seção própria do `sandmux stats` ([docs/docker.md](docker.md)).

## Voltar para a versão anterior

A versão anterior recusa as chaves `memory` e `processes` do `config.toml`, por serem desconhecidas. Retire-as antes de instalar o binário anterior.
