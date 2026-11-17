sandmux 0.1.20, antes da v1: o Docker privado confinado pelo kernel.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Confinamento do Docker privado por eBPF:** com o novo auxiliar root (`sandmux-helper`) e o BPF LSM ativo no kernel, o próprio kernel decide o que os contêineres de cada pasta podem abrir e alterar, mesmo com `--privileged`.
  - **Negados:** os segredos e o `file-deny` da pasta, com o conteúdo das pastas negadas, e as coberturas de `/etc`. Desmontar a cobertura ou criar um link não adianta, porque a decisão é pelo arquivo.
  - **`.git` protegido:** hooks, `config` e `config.worktree` não são gravados, e `commondir` não é criado.
  - Vale só para o Docker da pasta: o agente e o resto da máquina não são afetados, e não há custo mensurável.
  - Sem o auxiliar, o Docker sobe como antes, e `sandmux doctor` e `sandmux stats` dizem por quê.
  - Detalhes em [docs/helper.md](https://github.com/alanyoshida/sandmux/blob/main/docs/helper.md) e [docs/docker.md](https://github.com/alanyoshida/sandmux/blob/main/docs/docker.md#confinamento-por-ebpf).
- **Instalação do auxiliar:**
  - o `.deb`, o `.rpm` e o pacote do AUR ligam o socket do serviço na instalação;
  - pelo `install.sh` com `sudo`, o `sandmux setup` instala as unidades depois de perguntar;
  - o serviço só tem as capacidades `CAP_BPF` e `CAP_PERFMON`.

Esta versão muda o protocolo entre o cliente e o daemon (39). Depois de atualizar, rode `sandmux setup` (para o auxiliar) e `sandmux daemon restart`, e abra a TUI de novo.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
