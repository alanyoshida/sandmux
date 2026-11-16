## Why

O `sandmux doctor` só existe na linha de comando. Quem usa a TUI precisa sair dela, ou abrir uma aba de shell, para ver se a máquina tem tudo o que o sandbox usa, se o daemon e o cofre estão bem e se há chaves reais dentro do sandbox. Para mover essas chaves ao cofre, ainda precisa do `sandmux doctor --fix` no terminal, que pergunta o domínio de cada chave sem domínio conhecido. Pedido do usuário (2026-11-16): uma tela do doctor na página de configurações.

## What Changes

- **Seção Diagnóstico:** a página de configurações ganha a seção "Diagnóstico", depois do Cofre, no mesmo molde dele. As verificações do `sandmux doctor` rodam ao entrar na seção (levam cerca de 70 ms) e `r` roda de novo. O nome da seção na lateral mostra quantas verificações falharam (`✗1`).
- **Lista com detalhe:** uma linha por verificação, com `✓`, `✗` ou `·` (informação) e um resumo curto, agrupadas em Sandbox, Instalação, Configuração, Daemon, Cofre e Chaves reais. A área de baixo mostra a causa e a correção da linha escolhida. O comando longo do perfil do AppArmor fica no `sandmux doctor` do terminal, e a tela diz isso.
- **Atalho próprio:** a ação `doctor` (padrão `D`, configurável em `[tui.keys]`) abre a página direto na seção. A ação entra na ajuda e no menu de ações.
- **Mover as chaves pela TUI:** a tecla `f` faz o `doctor --fix` por dentro da TUI. Para cada chave real, um campo com os domínios vem preenchido pela inferência e pode ser editado. Enter confirma, e Esc pula o arquivo inteiro. O login do Google do `agy` pede uma confirmação. As chaves de um arquivo só vão ao cofre depois de o usuário confirmar todas. Com o cofre trancado, `f` pede a senha antes; sem cofre, aponta a seção Cofre.
- **Inferência a mais:** quando a inferência atual (nome da chave, provedor, `baseURL`) não acha domínio, a tela sugere o host da URL mais próxima da chave no mesmo arquivo. Isso é só uma sugestão no campo, que o usuário confirma; o vigia de chaves do daemon continua sem ela.
- **Diagnóstico como dados:** o `doctor` passa a montar um relatório (seções e verificações) que o terminal imprime e a TUI desenha. A saída do `sandmux doctor` no terminal não muda.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `terminal-ui`: a seção Diagnóstico na página de configurações e a ação `doctor`.
- `secret-vault`: mover as chaves reais ao cofre pela seção Diagnóstico, com o domínio editável e a sugestão pela URL próxima.

## Impact

- `crates/sandmux/src/doctor.rs`: o relatório em dados, a impressão no terminal a partir dele e os itens que dá para mover.
- `crates/sandmux/src/secretdomains.rs`: a sugestão pela URL mais próxima.
- `crates/sandmux/src/tui/settings.rs`, `render.rs` e `mod.rs`: a seção, as teclas, o fluxo de mover e o atalho.
- `crates/sandmux-policy/src/config.rs`: a ação `doctor` em `TUI_ACTIONS`.
- `docs/tui.md`, `docs/vault.md` e os pares em inglês.
- Sem mudança de protocolo: a TUI usa as mensagens do cofre que já existem.
