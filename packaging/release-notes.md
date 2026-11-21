sandmux 0.1.28, antes da v1: o escopo do sandbox sobrevive a um reinício do auxiliar root.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Correção

- **Ubuntu 24.04 com o kernel 6.8:** quando o [auxiliar root](https://github.com/alanyoshida/sandmux/blob/main/docs/helper.md#escopo-do-sandbox) reiniciava (uma atualização, um travamento), as abas que rodavam perdiam o escopo do sandbox sem aviso. Perdiam o bloqueio dos sockets abstratos de fora, dos sinais para fora e para o processo 1, e do `ioctl` em dispositivo. Agora o daemon congela cada aba, registra o escopo de novo no auxiliar seguinte e descongela; a aba segue de onde parou. Sem o escopo de volta em 30 s, a aba para. Num `sandmux run` sem terminal, o próprio lançador registra de novo.
- **Lançamento durante a troca do auxiliar:** podia ser recusado com "o auxiliar root não respondeu ao pedido". Agora o lançador tenta de novo por até 30 s, e o auxiliar, ao sair, não atende conexões novas. No 6.8, os programas do escopo carregam quando o auxiliar sobe e ficam carregados por 60 s depois da última aba, porque carregá-los leva segundos nesse kernel.

Nos kernels 6.12 ou mais novos (Landlock ABI 6), o escopo não depende do auxiliar, e nada muda.

Esta versão muda o lançador, o daemon e o auxiliar root. O protocolo entre o cliente e o daemon sobe para 44: depois de atualizar, rode `sandmux daemon restart`, e atualize o auxiliar (`sandmux setup`).

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
