# Tasks

## 1. Base

- [x] 1.1 Acrescentar `ratatui` (0.30), `crossterm` (0.29), `arboard` (com `wayland-data-control`) e `png` às dependências do binário `sandmux`; verificar com `cargo build`
- [x] 1.2 Implementar os modos da aba para o terminal do usuário (`host_modes` e `host_reset`), juntando o `ModeTracker` (kitty, foco) e o estado do `vt100` (colagem entre marcadores, cursor e teclado de aplicação, modo e codificação do mouse), sem tela alternativa nem mouse; verificar com testes de unidade
- [x] 1.3 Ler e validar `[tui]` no `config.toml` (`prefix`, `[tui.keys]`, teclas reservadas, ações repetidas e desconhecidas, com a linha no erro), sem efeito na política; verificar com testes de unidade dos cenários de "Seções do arquivo" e "Atalhos configuráveis"

## 2. Renomear

- [x] 2.1 Acrescentar `Msg::Rename`, validar o nome no daemon e gravá-lo na lista de sessões, subir `PROTOCOL` para 5, e implementar `sandmux rename <id> <nome>`; verificar com testes de CLI (nome novo sobrevive ao `daemon restart`, nome vazio recusado)

## 3. Núcleo da TUI

- [x] 3.1 Implementar o reconhecedor de entrada da TUI: prefixo configurável (byte e kitty), ações, sequências de mouse SGR e teclas dos modais (setas, Enter, Esc, Backspace, texto); verificar com testes de unidade
- [x] 3.2 Implementar o laço principal: threads de entrada, de conexão por aba e de tique, canal de eventos, emulador `vt100` e `ModeTracker` por aba, redesenho limitado e SIGWINCH com `Resize` em todas as abas; abrir com `sandmux` sem subcomando ou `sandmux tui`, e recusar sem terminal. Verificar com teste de PTY que a TUI abre com as abas existentes e com teste de CLI sem terminal
- [x] 3.3 Desenhar a barra de abas e a tela da aba ativa (cores 16, 256 e cor verdadeira, atributos, cursor, tela alternativa), com a conversão das células testada em unidade; verificar com teste de PTY que o texto colorido e a barra aparecem
- [x] 3.4 Repetir os modos da aba ativa no terminal do usuário e desligá-los ao trocar e ao sair; verificar com teste de PTY que, ao sair, o terminal do teste fica sem tela alternativa, sem mouse e sem o protocolo do kitty

## 4. Atalhos e modais

- [x] 4.1 Implementar a navegação (`n`, `p`, dígitos), o envio do prefixo e o descarte de tecla desconhecida; verificar com teste de PTY
- [x] 4.2 Implementar o modal de nova aba (harness, pasta, perfil) com as verificações do `run`, a pergunta de importação e os erros no modal; verificar com testes de PTY de aba criada em outra pasta e de pasta perigosa recusada
- [x] 4.3 Implementar os modais de lista, renomear, fechar com confirmação e ajuda; verificar com testes de PTY
- [x] 4.4 Implementar a aba encerrada (última tela, código, teclas) e parada (motivo), e o reinício com as variáveis de `pass-env` do ambiente; verificar com teste de PTY de `exit 3` e Ctrl+b r
- [x] 4.5 Atualizar a lista de abas pelo tique (sessão criada por outro cliente aparece, sessão encerrada por `sandmux kill` some); verificar com teste de PTY

## 5. Mouse e colagem

- [x] 5.1 Implementar o clique na barra, o repasse ao harness com as coordenadas da área (SGR e X10, movimentos só com 1003), a roda no histórico e a seleção com cópia em OSC 52; verificar com testes de PTY que escrevem sequências de mouse e conferem a aba ativa, a sequência recebida por um programa de teste na sessão e o OSC 52 na saída crua
- [x] 5.2 Implementar a colagem de imagem (`arboard`, PNG na pasta de entrada, caminho escrito na aba, avisos sem imagem e sem clipboard); verificar com teste de unidade da gravação do PNG e teste de PTY sem clipboard

## 6. Documentação

- [x] 6.1 Escrever `docs/tui.md` (abrir, barra, atalhos e `[tui]`, modais, aba encerrada, mouse e seleção, colagem, limites) e atualizar o README e o `docs/config.md`; verificar com o teste de documentação, que confere os atalhos padrão contra o código

## 7. Verificação final

- [x] 7.1 Rodar `./scripts/check.sh` sem falhas e sem daemon de teste sobrando
- [x] 7.2 Verificar à mão a fidelidade com o opencode e o hermes reais na TUI (tela alternativa, teclado do kitty, mouse, colagem de texto, cor verdadeira, troca de abas, sair e voltar), registrando o resultado no roadmap
