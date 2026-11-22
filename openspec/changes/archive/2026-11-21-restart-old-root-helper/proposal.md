## Why

Depois de instalar a 0.1.33 pelo `install.sh`, o usuário continuou vendo o `sleep` que rodou dentro do opencode na raiz da árvore de processos (2026-11-21). O `sandmux doctor` já avisava que o auxiliar root em execução era o 0.1.30, mas o auxiliar não tinha sido reiniciado. Três falhas se somam:

- **O `sandmux setup` não reinicia o auxiliar numa atualização.** Ele só mexe no auxiliar quando o arquivo da unidade do systemd muda. Numa atualização só o binário muda, então o setup diz "as unidades já estão instaladas" e nunca chega ao `systemctl try-restart`. O `install.sh` também não reinicia o auxiliar: os pacotes `.deb` e `.rpm` reiniciam.
- **O aviso da 0.1.33 manda rodar o setup**, que não resolve esse caso.
- **A árvore depende do pai vindo no evento.** O auxiliar só manda o pai de cada execução a partir da 0.1.32, e o daemon põe na raiz uma execução que chega sem ele, embora já leia o `/proc/<pid>/stat` do processo, que traz o pai.

## What Changes

- **`install.sh`:** com privilégio e as unidades do auxiliar instaladas, reinicia o auxiliar depois de gravar os binários novos, como os pacotes já fazem.
- **`sandmux setup`:** com as unidades em dia e o auxiliar em execução mais antigo que o sandmux, reinicia o auxiliar (`systemctl try-restart`), perguntando antes de usar o `sudo`. Com `--no-sudo`, mostra o comando.
- **Aviso de auxiliar antigo:** no `sandmux doctor` e na seção Processos, a correção passa a citar também `sudo systemctl restart sandmux-helper.service`, que vale em qualquer instalação. A seção Processos deixa de dizer que a árvore sai sem os pais, porque deixa de ser verdade.
- **Árvore de processos:** quando o evento de execução chega sem o pai, o daemon usa o pai lido no `/proc`. A árvore sai certa também com um auxiliar de antes da 0.1.32.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `installation`: o `install.sh` e o `sandmux setup` reiniciam o auxiliar root antigo.
- `root-helper`: a correção citada no aviso de auxiliar antigo.
- `process-tree`: o pai pelo `/proc` quando o evento não o traz, e o texto do aviso na seção.

## Impact

- **`sandmux`:** `setup.rs` (o passo novo do auxiliar), `doctor.rs` e `tui/render.rs` (o texto do aviso), `daemon/proctree.rs` (o pai pelo `/proc`).
- **`packaging/install.sh`:** o `try-restart` depois de instalar.
- **Testes e documentação:** testes de unidade do passo do setup e da árvore; os testes do aviso com o texto novo; `docs/helper.md`, `docs/install.md`, `docs/tui.md` e as versões em inglês.
- Sem protocolo novo e sem mudança no auxiliar root.
