sandmux 0.1.11, antes da v1: histórico de tela cifrado em disco.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Histórico de tela em disco, opcional e cifrado:** com `screen-history = true` em `[global]`, ou o item Histórico em disco na seção Geral das configurações, a saída das abas sobrevive ao reinício do daemon e a uma queda.
  - Vai para o disco a cada 2 segundos, cifrada com XChaCha20-Poly1305 e uma chave aleatória guardada no cofre. Sem cofre, nada é gravado.
  - Com o cofre trancado, a saída espera na memória (até 2 MiB por aba) e é gravada quando o cofre abre.
  - A aba relançada recebe o histórico de volta, com a linha "sandmux: a aba voltou". Com o cofre trancado, ele volta no desbloqueio.
  - Fechar a aba apaga o arquivo dela, e desligar a opção apaga todos.

  Vem desligado: a saída pode mostrar segredos que o agente imprimiu, e por padrão nada dela vai para o disco. Detalhes em [docs/sessions.md](https://github.com/alanyoshida/sandmux/blob/main/docs/sessions.md#histórico-em-disco).

Depois de atualizar, rode `sandmux daemon restart`.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
