sandmux 0.1.37, antes da v1: um lançamento logo depois de um reinício do auxiliar root não é mais recusado.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Correção

- **Lançamento durante a troca do auxiliar root:** num kernel com o Landlock abaixo do ABI 6 (como o 6.8 do Ubuntu 24.04), o lançamento verifica o [auxiliar root](https://github.com/alanyoshida/sandmux/blob/main/docs/helper.md) antes de subir o sandbox. Essa verificação fazia uma tentativa só. Ao sair (um `systemctl restart`, que o `sandmux setup` faz numa atualização), o auxiliar velho fecha as conexões novas sem responder por alguns segundos. Um lançamento nesse intervalo era recusado com "o auxiliar root não respondeu ao pedido". Agora a verificação tenta de novo, como o registro do escopo já fazia: por até 30 s sem resposta, e por 1 s sem o socket do auxiliar.

Esta versão muda só o lançador (`sandmux-run`). O protocolo e o auxiliar root não mudam.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
