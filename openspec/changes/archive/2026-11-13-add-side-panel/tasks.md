# Tasks

## 1. Resumo da política

- [x] 1.1 Criar `PolicySummary` no `sandmux-proto`, preencher em `prepare`, guardar no daemon por lançamento e enviar no `StatsOf`; subir `PROTOCOL` para 6. Verificar com teste de serialização e com teste de CLI em que `sandmux stats` mostra o KVM e uma pasta extra do resumo

## 2. Painel

- [x] 2.1 Acrescentar `panel` em `[tui]` e a ação `panel` (tecla `s`); verificar com testes de unidade da leitura
- [x] 2.2 Desenhar o painel (Proteção e Recursos), encolher a área do terminal e refazer o tamanho das abas ao ligar, desligar ou redimensionar, e pedir `Stats` da aba ativa a cada tique e ao trocar de aba. Verificar com testes de PTY dos cenários de "Painel lateral" (120 colunas, Ctrl+b s, 80 colunas) e de "Proteção no painel"

## 3. Documentação e verificação

- [x] 3.1 Documentar o painel em `docs/tui.md` e o resumo em `docs/limits.md`; verificar com o teste de documentação
- [x] 3.2 Rodar `./scripts/check.sh` sem falhas
