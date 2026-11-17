# Auxiliar root (`sandmux-helper`)

> English: [helper.md](en/helper.md)

O `sandmux-helper` é o único pedaço do sandmux que roda como root. Ele existe para uma coisa: prender ao cgroup do [Docker privado](docker.md) de uma pasta os programas BPF LSM que decidem, no kernel, o que os contêineres podem abrir e alterar. Sem ele, o resto do sandmux funciona igual, e o Docker privado sobe só com as proteções sem privilégio.

## O que ele faz

O daemon pede o confinamento quando o Docker privado de uma pasta sobe, e o auxiliar faz duas coisas:

1. Carrega os programas e os prende à slice `sandmux-docker-<id>.slice` daquela pasta.
   - Eles valem para o `dockerd`, o `containerd`, o `runc` e todo contêiner da pasta.
   - Não valem para nenhum outro processo da máquina, nem para o agente.
2. Marca os arquivos que o daemon manda:
   - **negados:** os segredos e o `file-deny` da pasta, com tudo o que existe dentro das pastas negadas, e as coberturas de `/etc`;
   - **protegidos contra escrita:** `.git/hooks`, `.git/config` e `.git/config.worktree`;
   - **diretórios git:** neles não dá para criar `commondir`, `hooks`, `config` nem `config.worktree`.

A decisão é pelo arquivo (o inode), e não pelo caminho. Por isso, desmontar uma cobertura, tirar o somente-leitura de um bind, renomear ou criar um link não abrem caminho até o arquivo. Os detalhes do que fica negado estão em [docker.md](docker.md#confinamento-por-ebpf).

## Quem pode pedir o quê

- O socket é `/run/sandmux-helper.sock`, e qualquer usuário conecta. O auxiliar sabe quem pede pelas credenciais do socket (`SO_PEERCRED`), e não por algo que o pedido diga.
- Um usuário só confina um cgroup cujo diretório pertence a ele. O root pode qualquer um.
- Uma regra só nega. Ela nunca dá acesso, e só vale para os processos do cgroup pedido. Marcar um arquivo do sistema, como `/etc/hostname`, só impede o Docker daquele usuário de abri-lo.
- O auxiliar nunca abre um caminho. Os arquivos chegam como descritores abertos pelo próprio usuário, que prova assim que alcança o arquivo.
- **Limites por usuário:** 16 confinamentos ativos, cada um com até 16384 marcas. Um pedido acima disso é recusado, sem afetar os outros.

## Quanto tempo as regras duram

As regras e as marcas valem enquanto a conexão do daemon com o auxiliar estiver aberta. Elas somem quando:

- o Docker da pasta para;
- o daemon termina, mesmo morto por `SIGKILL`;
- o auxiliar para.

Se o auxiliar parar com o Docker rodando, o daemon para o Docker da pasta, que não segue sem as regras. O próximo `docker` sobe outro Docker e liga o confinamento de novo, se o auxiliar responder. O socket do systemd sobe o auxiliar de volta na conexão seguinte.

## Instalar

O auxiliar precisa de duas coisas:

- o kernel com o BPF LSM ativo, isto é, `bpf` na lista de `/sys/kernel/security/lsm`;
- o binário numa pasta do root.

**`sandmux setup`** grava as unidades `sandmux-helper.socket` e `sandmux-helper.service` em `/etc/systemd/system` e liga o socket. Ele usa `sudo` depois de perguntar, como faz com o perfil do AppArmor ([install.md](install.md#sandmux-setup)).

Ele recusa instalar quando o binário, ou uma pasta acima dele, pode ser alterado por um usuário que não seja o root. É o caso de `~/.local/bin`, onde o `install.sh` põe os binários sem `sudo`. O motivo: o serviço roda como root, e quem altera o binário ganharia root.

Com `--no-sudo`, o setup mostra os comandos para rodar como root:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now sandmux-helper.socket
```

**Pacotes:** o `.deb`, o `.rpm` e o pacote do AUR trazem as unidades em `/usr/lib/systemd/system` e ligam o socket na instalação. A fórmula do Homebrew não traz o auxiliar, porque o Homebrew não roda como root.

**Kernel sem `bpf` na lista:** acrescente `bpf` ao parâmetro `lsm=` do boot, mantendo os que já estão em `/sys/kernel/security/lsm`. Depois de reiniciar, `sandmux doctor` mostra a verificação "confinamento do Docker por eBPF".

## Desligar

```bash
sudo systemctl disable --now sandmux-helper.socket
```

O Docker de cada pasta volta às proteções sem privilégio na próxima vez que subir. `install.sh --uninstall` para e remove as unidades.

## Como saber se vale

- `sandmux doctor` mostra a verificação "confinamento do Docker por eBPF", com o motivo e a correção quando falta algo.
- `sandmux stats <id>` mostra, na seção do Docker privado, "confinamento eBPF: ligado" ou "desligado:" com o motivo. O painel da TUI mostra a linha "eBPF".

## O que o serviço pode fazer

A unidade do serviço tem as restrições do systemd:

- **capacidades:** só `CAP_BPF` e `CAP_PERFMON`, as que carregar e prender programas BPF LSM a um cgroup exige;
- **sistema de arquivos:** `ProtectSystem=strict` e `ProtectHome=yes`;
- **rede:** nenhuma (`PrivateNetwork=yes`), e só sockets Unix.

Os programas são montados pelo próprio sandmux, a partir do BTF do kernel que está rodando (`/sys/kernel/btf/vmlinux`), sem `clang` e sem código compilado de fora.

## Testes

Os testes que exigem root rodam por `scripts/check-root.sh`, fora do `check.sh`:

```bash
sudo scripts/check-root.sh
```

O script sobe o auxiliar como root num socket temporário e roda `crates/sandmux/tests/docker_ebpf.rs` como o usuário que chamou o `sudo`. Ele roda binários compilados por esse usuário. Por isso, liberá-lo no `sudoers` sem senha dá root a quem roda como esse usuário. Se fizer isso, faça só enquanto precisa.
