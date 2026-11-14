# Tasks

## 1. Especificação e configuração

- [x] 1.1 Acrescentar a seção `network` à `LaunchSpec` (modo, pasta do registro, etiqueta, pasta de trabalho, nameservers do host) e `network`, `net-log-days` e `net-log-max` à configuração (as duas últimas só em `[global]`), com `--network` no `run`; verificar com testes de unidade da leitura e da precedência
- [x] 1.2 Compor a rede: `resolv.conf` gerado (no destino do link em `/run` ou por cima do arquivo), `SANDMUX_HOST`, seção `network` e resumo da política; verificar com testes de unidade da composição nos dois formatos de `resolv.conf`

## 2. Gateway

- [x] 2.1 Criar o crate `sandmux-net` com o dispositivo TUN assíncrono, o `ipstack`, o encaminhamento TCP (com `10.0.2.254` para o `127.0.0.1` do host e o SNI e o `Host`) e UDP, os bloqueios fixos e o DNS (parser, nome especial, repasse, ligação de IP a domínio); verificar com testes de unidade do parser de DNS, do SNI e do `Host`
- [x] 2.2 Implementar o registro (linhas JSON por dia, `new` pela lista de domínios conhecidos da pasta); verificar com teste de unidade

## 3. Lançador

- [x] 3.1 Criar o namespace de rede com `CLONE_NEWNET`, configurar `lo` e `sandmux0` (endereço, máscara, rota padrão) no filho antes de `setup_root`, passar o TUN pelo `socketpair` e subir o gateway no supervisor; verificar com testes de integração sem terminal:
  - as interfaces são só `lo` e `sandmux0`;
  - um servidor TCP do host é alcançado por `host.sandmux.internal`, com a linha no registro;
  - o DNS responde o nome especial e `localhost`;
  - os metadados de cloud e o QUIC são bloqueados e registrados;
  - o modo `sem-rede` falha na hora.
- [x] 3.2 Rodar a suíte de fuga e os testes existentes com a rede nova, ajustando o que dependia da rede do host; verificar com `./scripts/check.sh`

## 4. Daemon, TUI e linha de comando

- [x] 4.1 Reservar o identificador da sessão antes do lançamento, gerar as etiquetas e guardá-las na lista de sessões; acrescentar o pedido `Net` (protocolo 7) e `sandmux net [<id>]`; verificar com teste de PTY de uma sessão que acessa um servidor local e com `sandmux net 1`
- [x] 4.2 Fazer a retenção no daemon (ao subir e de hora em hora); verificar com teste de integração de um arquivo de 40 dias apagado
- [x] 4.3 Acrescentar a seção Rede ao painel e a marca de domínio novo na barra; verificar com teste de PTY
- [x] 4.4 Atualizar o `doctor` (rede com namespace próprio e gateway) e o resumo da política; verificar com teste de CLI

## 5. Documentação e verificação

- [x] 5.1 Escrever `docs/network.md` e atualizar o README, `docs/tui.md` e `docs/config.md`; verificar com o teste de documentação
- [x] 5.2 Rodar `./scripts/check.sh` sem falhas
- [x] 5.3 Verificar à mão uma sessão real do opencode com a rede pelo gateway (a interface sobe e acessa o provedor, se configurado), um `curl` de um arquivo grande com a velocidade anotada, e o painel com os domínios
