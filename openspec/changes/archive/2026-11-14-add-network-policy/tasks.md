# Tasks

## 1. Política

- [x] 1.1 Criar os padrões de destino (exato, curinga, IPv4 e CIDR) com validação e casamento sem diferença de maiúsculas, e os três catálogos embutidos (exfiltração, domínios comuns por grupo, provedores de inferência); verificar com testes de unidade dos formatos aceitos e recusados e do casamento
- [x] 1.2 Acrescentar `deny`, `allow`, `allow-off`, `catalog` e `catalog-off` às camadas, os modos `inferencia` e `custom` à chave `network`, e `--deny` e `--allow` ao `run`; aceitar as chaves em `sandmux config add|remove`; verificar com testes de unidade da leitura, dos erros (padrão inválido, grupo desconhecido, modo inválido) e da soma entre camadas
- [x] 1.3 Compilar a política de rede (`NetPolicy` na `LaunchSpec`, com modo e regras com origem e estado) e o caminho do arquivo de política; verificar com testes de unidade da compilação nos quatro modos, com itens desligados e com "negar sempre vence"

## 2. Gateway

- [x] 2.1 Ligar cada IP a vários nomes, decidir DNS (NXDOMAIN), TCP e UDP pela política, com os motivos no registro; verificar com testes de unidade da decisão (SNI forjado, CDN com vários nomes, curinga, faixa CIDR) e do NXDOMAIN
- [x] 2.2 Reler o arquivo de política a cada 250 ms no máximo, mantendo a anterior quando o arquivo é inválido; verificar com teste de unidade
- [x] 2.3 Testes de integração sem internet, pelo `run`: Host negado no modo aberta, item do catálogo bloqueado e desligado por `catalog-off`, modo custom com `host.sandmux.internal` liberado e outro destino bloqueado, NXDOMAIN no modo só inferência, SNI forjado e faixa CIDR

## 3. Daemon e linha de comando

- [x] 3.1 Gravar o arquivo de política de cada lançamento, recompilar quando o arquivo de configuração muda (verificação a cada segundo) e apagar no fim; verificar com teste de PTY de uma sessão bloqueada em até 2 segundos depois de `sandmux config add deny ... --global`
- [x] 3.2 Acrescentar `deny` e `allow` às `LaunchOptions`, o pedido `NetEdit` (protocolo 8) e `sandmux net deny|allow|forget <padrão> <id>`, com o aviso de `sem-rede`; verificar com teste de PTY de liberar na aba e de a regra continuar depois de `sandmux daemon restart`
- [x] 3.3 Implementar `sandmux net rules [<id>]` com origem e estado de cada regra, e o modo e a contagem de regras no `config show` e no resumo da política; verificar com teste de CLI

## 4. TUI

- [x] 4.1 Acrescentar a marca `!` de bloqueio na barra e o bloqueio na seção Rede do painel; verificar com teste de PTY
- [x] 4.2 Criar a tela de rede (Ctrl+b `a`): acessos e regras, negar e liberar com a escolha da camada, ligar e desligar itens, trocar o modo; verificar com testes de PTY de negar na aba, liberar no projeto e trocar o modo

## 5. Documentação e verificação

- [x] 5.1 Atualizar `docs/network.md` (modos, catálogos, padrões, decisão, efeito imediato, regras da aba, tela de rede), `docs/config.md`, `docs/tui.md`, `docs/sessions.md` e o README; verificar com o teste de documentação, que confere os catálogos documentados contra os embutidos
- [x] 5.2 Rodar `./scripts/check.sh` sem falhas
- [x] 5.3 Verificar à mão uma sessão real do opencode no modo só inferência (o modelo responde, o `registry.npmjs.org` é bloqueado) e no modo custom com `catalog = ["opencode"]` e o OpenRouter liberado
