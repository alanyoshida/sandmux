sandmux 0.1.33, antes da v1: avisa quando o auxiliar root em execução é mais antigo que o sandmux.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidade

- **Aviso de auxiliar root antigo:** o [auxiliar root](https://github.com/alanyoshida/sandmux/blob/main/docs/helper.md#instalar) só troca ao ser reiniciado, e atualizar os binários não o reinicia. Quando o auxiliar em execução é de uma versão anterior à do sandmux, o `sandmux doctor` mostra `[aviso] versão do auxiliar root`, com as duas versões e a correção, e a seção "Processos" da TUI mostra o mesmo aviso no topo. Antes, nada avisava: com o auxiliar da 0.1.30 e o sandmux 0.1.32, a árvore de processos saía com todos os processos na raiz, sem os pais, e o `doctor` mostrava a versão do auxiliar como `[ok]`. O aviso não conta como falha.

Esta versão muda só o cliente. O protocolo entre o cliente e o daemon não muda, e o auxiliar root não muda de comportamento. Mesmo assim, depois de atualizar, rode `sandmux setup`: ele reinicia o auxiliar. Até lá o aviso novo aparece, porque o auxiliar em execução continua sendo o da versão anterior.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
