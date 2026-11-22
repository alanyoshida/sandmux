sandmux 0.1.34, antes da v1: o grafo do cofre, e a árvore de processos e o auxiliar root em dia depois de atualizar.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Grafo do cofre:** na seção Cofre da página de configurações, `g` abre uma tela cheia com os domínios donos à esquerda, o domínio escolhido no centro com os segredos dele em volta, e os detalhes do segredo em foco à direita. O valor de um segredo nunca aparece. Tem zoom (`+` e `-`), movimento (`h` `j` `k` `l` e o mouse), filtro (`/`), `o` para o dono seguinte de um segredo de vários domínios, e edição pelas teclas da seção Cofre. As conexões de banco aparecem num grupo só de leitura. Para fontes sem Braille, `graph-edges = "blocos"` em `[tui]`. Detalhes em [docs/vault.md](https://github.com/alanyoshida/sandmux/blob/main/docs/vault.md#grafo-do-cofre).

## Correções

- **Auxiliar root antigo depois de atualizar:** o auxiliar em execução só troca ao ser reiniciado, e até aqui o `install.sh` e o `sandmux setup` não o reiniciavam numa atualização. Agora o `install.sh` com `sudo` reinicia o auxiliar depois de gravar os binários, e o `sandmux setup` reinicia o auxiliar mais antigo que o sandmux, perguntando antes do `sudo`. O aviso da 0.1.33 recomendava o `sandmux setup`, que não resolvia; agora cita também `sudo systemctl restart sandmux-helper.service`.
- **Árvore de processos sem os pais:** com um auxiliar de antes da 0.1.32, os processos executados depois que a aba abria apareciam na raiz da árvore. Agora o daemon pega o pai no `/proc` quando a execução chega sem ele.

Esta versão muda o cliente e o daemon; o protocolo e o auxiliar root não mudam. Pelo `install.sh` com `sudo`, o auxiliar é reiniciado na instalação. Por outro caminho, rode `sandmux setup` depois de atualizar.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
