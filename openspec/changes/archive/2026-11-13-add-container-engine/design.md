# Design

## Context

Veja proposal.md, seção Why. Estado e fatos que importam:

- **Lançamento do sandbox.** `sandmux_policy::compose` monta a especificação: montagens, Landlock e seccomp. O `sandmux-run` cria os namespaces (user, mount, pid, ipc, uts), monta a raiz nova com `pivot_root` a partir das montagens da especificação, aplica Landlock e seccomp e executa o comando.
- **O daemon** lança cada sessão num escopo do systemd com os tetos do change 4, e sabe a pasta de trabalho, as opções e a configuração resolvida de cada sessão.
- **A máquina de referência** tem `dockerd` 29.9.3, `rootlesskit`, `containerd`, `newuidmap`, o plugin `docker-compose` e a faixa `100000:65536` em `/etc/subuid` e `/etc/subgid`. Não tem `slirp4netns` nem `pasta`.
- **Prova de 2026-11-13.** O `rootlesskit` foi chamado com `--net=gvisor-tap-vsock`, `--port-driver=builtin`, `--copy-up=/etc`, `--copy-up=/run`, `--propagation=rslave` e `--pidns`. O processo filho montou, dentro do namespace do `rootlesskit`:
  - um tmpfs como raiz;
  - `/usr` e `/etc` em somente leitura, e os links de `/bin` e `/lib`;
  - `/dev` e `/sys` do host;
  - um `/proc` novo e `/tmp` e `/run` em tmpfs;
  - a pasta de trabalho, os dados do Docker, a pasta do socket, a pasta de estado e a pasta de estado do `rootlesskit`, esta última necessária para o driver de portas.

  Depois fez `pivot_root` e executou o `dockerd`. O resultado:
  - build e execução funcionaram;
  - `-v ~/.ssh` e um link para `~/.ssh` chegaram vazios ao contêiner;
  - a rede funcionou;
  - a porta publicada saiu em `0.0.0.0`.
- **Filtrar a API do Docker num proxy foi descartado.** O `dockerd` resolve os caminhos no momento de iniciar o contêiner, na visão de arquivos dele. Entre a checagem e o uso, o agente pode trocar uma pasta por um link simbólico para fora. Uma visão restrita resolve isso por construção.

## Goals / Non-Goals

**Goals:**

- A visão do `dockerd` vem da mesma composição do sandbox. Uma cobertura nova em `/etc` vale para os dois.
- Nenhum processo do Docker privado sobrevive às sessões da pasta nem ao daemon.
- Os testes não baixam imagem.

**Non-Goals:**

- Cache de downloads de imagem entre pastas (change 8).
- Política de rede dos contêineres (etapa 4).
- Docker privado no `run` sem terminal.
- Outros motores (podman), registry próprio e instalação do Docker pelo sandmux (pós-v1, já no roadmap).

## Decisions

### 1. Visão do `dockerd` pela composição do sandbox

`sandmux_policy::compose_docker_view(host, workdir, paths)` devolve uma especificação só de montagens, com o mesmo código da composição do sandbox:

- pastas de sistema em somente leitura, com as coberturas de `/etc` e os `passwd` e `group` gerados;
- a pasta de trabalho com escrita, com a proteção do `.git` (hooks e config em somente leitura, inclusive no worktree);
- a pasta de entrada em somente leitura;
- dispositivos: o `/dev` do host inteiro, porque o `runc` cria os nós dos contêineres a partir dele. Dentro do user namespace do `rootlesskit`, os nós continuam com as permissões do host;
- `/sys` do host, que o `runc` usa para o cgroup;
- `/proc` novo, porque o `rootlesskit` cria um pid namespace;
- `/tmp` e `/run` em tmpfs;
- as pastas do Docker com escrita: dados, estado, socket e o estado do `rootlesskit`.

Não entram o HOME (real ou sintético), as pastas de `--ro` e `--rw` das sessões nem os caches e toolchains. A visão é da pasta, e as sessões da pasta podem ter opções diferentes. Fica documentado como limite: um contêiner não vê `~/.cache` nem as pastas extras de uma sessão.

Alternativa descartada: dar ao `dockerd` a visão de uma sessão. Sessões diferentes na mesma pasta dariam visões diferentes ao mesmo Docker.

### 2. `sandmux-run --view-file <arquivo>`

Modo novo do lançador, chamado como processo filho do `rootlesskit`, que já é root no user namespace dele e tem um mount namespace e um pid namespace próprios. A especificação vai num arquivo (modo 600) na pasta de estado, e não num descritor herdado, porque o `rootlesskit` não garante repassar descritores extras ao filho. Ela só tem montagens e não guarda segredo. O ambiente do comando é o da especificação, mais as variáveis `ROOTLESSKIT_*` e `_ROOTLESSKIT_*` herdadas, que o `dockerd` usa para entrar no modo sem root e achar o driver de portas. O modo:

1. lê a especificação;
2. monta a raiz nova com o mesmo código de montagens do sandbox;
3. faz `pivot_root`;
4. executa o comando com o ambiente da especificação.

Não cria namespaces, não aplica Landlock nem seccomp, e não faz `PR_SET_NO_NEW_PRIVS`. O `dockerd` precisa montar e criar namespaces para os contêineres, e o que o confina é a visão de arquivos e o user namespace sem privilégio do `rootlesskit`.

### 3. Subida pelo daemon

A cadeia de processos, com `<id>` sendo o hash FNV-1a de 64 bits do caminho canônico da pasta, em 16 caracteres hexadecimais:

```
systemd-run --user --scope --collect -p Delegate=yes -p OOMPolicy=continue \
    -p MemoryMax=... -p TasksMax=... --description="sandmux: docker <pasta>" --
  rootlesskit --state-dir=<estado>/rk --net=<driver> --mtu=<mtu> \
      --port-driver=builtin --propagation=rslave --pidns --
    sandmux-run --view-file <estado>/view.json
      dockerd --data-root <dados>/docker/<id> --host unix://<estado>/docker.sock \
          --exec-root <estado>/exec --pidfile <estado>/docker.pid \
          --ip 127.0.0.1 --exec-opt native.cgroupdriver=cgroupfs
```

- **Pasta de estado:** `$XDG_RUNTIME_DIR/sandmux/docker/<id>`, curta o bastante para os sockets.
- **Pasta da visão:** um arquivo `pasta` grava o caminho da pasta de trabalho, para diagnóstico.
- **Sem `--copy-up`:** a prova usou `--copy-up=/etc` e `--copy-up=/run`, mas a visão monta o próprio `/etc` e o próprio `/run`. O copy-up troca os arquivos de `/etc` por links simbólicos, e o lançador recusa montar por cima de um link, o que é uma proteção de propósito. Achado ao implementar, em 2026-11-13.
- **Driver de rede:** `slirp4netns` se houver (≥ 0.4), senão `pasta`, senão `gvisor-tap-vsock`. É a mesma ordem do `dockerd-rootless.sh`, sem o `vpnkit`. O MTU é 65520 para os três, como no script.
- **`--ip 127.0.0.1`:** as portas publicadas sem endereço vão para o localhost.
- **`Delegate=yes` e o driver `cgroupfs`:** os contêineres ficam em cgroups filhos do escopo. Os tetos e a medição do escopo cobrem o Docker inteiro.
  - O daemon guarda o processo do `systemd-run` e o caminho do cgroup do escopo.
  - Sem suporte a tetos, a cadeia começa direto no `rootlesskit`.
- **Thread fixa:** os processos do Docker nascem de uma thread fixa do gerente, porque o `PR_SET_PDEATHSIG` é ligado à thread que faz o fork. Nascendo da thread da conexão, que termina logo, o Docker recebia SIGTERM ao fim da primeira conexão. Achado ao implementar, em 2026-11-13.
- **Pronto:** o daemon espera o socket aceitar conexão, até 30 s. O log do `dockerd` vai para `<estado>/dockerd.log`, que nunca recebe saída de sessão.
- **Parada:** o daemon manda SIGTERM ao `rootlesskit`, espera 15 s e manda SIGKILL. O escopo leva junto o que sobrar, porque o daemon também chama `systemctl --user stop` no escopo, quando ele existe. Os pais dos processos usam `PR_SET_PDEATHSIG`, como nas sessões, para nada sobreviver ao daemon.

### 4. Socket da sessão e repasse

- **Socket da sessão:** para cada sessão, o daemon cria `$XDG_RUNTIME_DIR/sandmux/s/<id-da-sessão>.sock` (modo 600). A composição recebe esse caminho e o monta em `/run/sandmux/docker.sock` no sandbox, com `DOCKER_HOST` definido.
- **Landlock:** conectar num socket por caminho não depende de regra de Landlock. O socket do daemon de sessões continua fora, porque não está na visão.
- **Repasse:** uma thread aceita as conexões. Na primeira, o daemon sobe o Docker da pasta. Depois, conecta ao socket do `dockerd` e copia os bytes nos dois sentidos, com duas threads e `shutdown` ao fim de cada lado. Isso cobre `attach`, `exec`, `build` e as conexões que viram outro protocolo, sem interpretar HTTP.
- **Sem suporte, ou se a subida falha:** o daemon lê o cabeçalho do pedido e responde `HTTP/1.1 503` com `{"message": "..."}`. É o formato de erro da API, que o `docker` mostra como `Error response from daemon: ...`.
- **Fim da sessão:** quando a sessão termina, encerrada ou parada, o socket dela é apagado. Quando a pasta fica sem sessão viva, o Docker dela para.

### 5. Detecção de suporte

`docker::support()` confere:

- `rootlesskit`, `dockerd`, `containerd` e `newuidmap` no `PATH`;
- uma faixa do usuário em `/etc/subuid` e em `/etc/subgid`, pelo nome ou pelo UID;
- o driver de rede escolhido.

O `doctor` mostra o resultado. Com o `gvisor-tap-vsock`, avisa que o driver é experimental e sugere instalar `pasta` ou `slirp4netns`.

### 6. Medição

O `StatsOf` ganha `docker: Option<DockerStats { memory, memory_max, processes, processes_max, containers }>`, com os contadores do escopo e o número de contêineres rodando. Os contêineres são contados como os cgroups filhos que o `dockerd` cria com o prefixo `docker`; se o formato não bater, o daemon pede `GET /containers/json` pelo socket. `PROTOCOL` passa a 4.

### 7. Testes

- **Imagem:** os testes constroem uma imagem `FROM scratch` com o `busybox` estático do host (`/usr/bin/busybox`), sem baixar nada.
- **Pulam com mensagem** quando não há `busybox` estático ou Docker sem root.
- **Pasta de estado:** cada teste tem o próprio `XDG_RUNTIME_DIR`. A pasta de estado do Docker fica dentro dele, curta o bastante para os sockets: o `Env` dos testes passa a criar o runtime em `/run/user/<uid>/sxt-<n>` quando essa pasta existe.

## Risks / Trade-offs

- **[Contêiner escapa das regras de seccomp e Landlock do sandmux]** Um contêiner roda com o seccomp do Docker e no user namespace do `rootlesskit`, não com as regras do sandmux. → A visão de arquivos é a mesma do sandbox, o que impede o que importa: credenciais e o resto do HOME. O user namespace sem privilégio impede `bpf`, módulos e `mount` no host. Aceito e registrado no roadmap.
- **[Driver de rede experimental]** O `gvisor-tap-vsock` é marcado como experimental pelo `rootlesskit`. → Funcionou na prova. O `doctor` sugere `pasta`, e o driver melhor é usado quando existe.
- **[Visão por pasta, e não por sessão]** O contêiner não vê as pastas extras nem o HOME sintético. → Documentado. Quem precisa monta a pasta dentro da pasta de trabalho.
- **[Tempo de subida]** O `dockerd` leva de 1 a 3 s para subir. → A subida é sob demanda, e só o primeiro `docker` espera.
- **[Arquivos de subuid no disco]** As imagens criam arquivos com UIDs da faixa subordinada, que o usuário não apaga com `rm` comum. → `docs/docker.md` explica como apagar com `rootlesskit rm -rf`, ou com `docker system prune` dentro de uma sessão.

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart`, que o cliente pede por causa do protocolo 4. Nada é migrado: o Docker privado começa vazio em cada pasta.
