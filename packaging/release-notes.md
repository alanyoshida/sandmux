sandmux 0.1.23, antes da v1: o processo de cada conexão.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Processo de cada conexão:** cada conexão TCP e UDP de uma aba no registro de rede diz qual processo a abriu, com o pid e o executável. Dá para separar a chamada de inferência do harness de um `curl` lançado por uma ferramenta ou de um pacote baixado por um `npm install`. Os argumentos ficam no registro de execuções, pelo pid.
  - Com o auxiliar root (`sandmux-helper`) e o BPF LSM ativos, o processo das conexões TCP vem do kernel, registrado no `connect`.
  - Sem eles, e no UDP, o gateway acha o dono do socket no começo da conexão, sem root. Isso custa cerca de 1 ms por conexão. A linha diz a fonte (`proc_via`).
  - `sandmux net` ganha a coluna do processo, e a tela de rede da TUI mostra o processo da linha.
  - As linhas do Docker privado ficam sem processo.
  - Detalhes em [docs/network.md](https://github.com/alanyoshida/sandmux/blob/main/docs/network.md#processo-de-cada-conexão).

Esta versão muda o protocolo entre o cliente e o daemon (42), o auxiliar root e o lançador. Depois de atualizar, rode `sandmux setup` (que reinicia o auxiliar) e `sandmux daemon restart`, e abra a TUI de novo.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
