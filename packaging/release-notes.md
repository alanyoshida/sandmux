sandmux 0.1.19, antes da v1: o Docker privado de cada pasta mais fechado.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Chamadas de sistema negadas no Docker privado:** o `dockerd` sobe com um filtro seccomp do sandmux, herdado por todo contêiner da pasta, também com `--privileged` e `--security-opt seccomp=unconfined`.
  - Negadas: as mesmas do sandbox do agente (`bpf`, `perf_event_open`, módulos, `kexec`, `userfaultfd`, `io_uring` e os `ioctl`s que injetam teclas no terminal), menos as de montagem, que o Docker usa.
  - O chaveiro do kernel responde como se não existisse. O `su` e o `sudo` das imagens continuam funcionando.
- **`/dev` mínimo nos contêineres:** só `null`, `zero`, `full`, `random`, `urandom`, `tty`, pseudo-terminais e `/dev/shm` próprios. Os terminais do usuário, a câmera, o áudio e os outros dispositivos do host não entram; a GPU e o `/dev/kvm` seguem as chaves `gpu` e `kvm` da configuração da pasta.
- **Segredos da pasta cobertos:** quando o Docker da pasta sobe, os arquivos com cara de segredo e os caminhos de `file-deny` chegam vazios e em somente leitura a um `-v $PWD:/w`, menos os que o próprio agente criou. Acima de 4096 caminhos, o Docker não sobe e o `docker` explica como reduzir a lista.
- **cgroups próprios:** o Docker roda num cgroup namespace próprio, e um contêiner com `-v /sys/fs/cgroup:/c` só vê os cgroups do Docker da pasta, sem o teto, as sessões e o resto do usuário.
- Limites, como o contêiner privilegiado que consegue tirar as coberturas, em [docs/docker.md](https://github.com/alanyoshida/sandmux/blob/main/docs/docker.md#limites).

Esta versão não muda o protocolo entre o cliente e o daemon. Depois de atualizar, rode `sandmux daemon restart` para o Docker de cada pasta subir com a visão nova.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
