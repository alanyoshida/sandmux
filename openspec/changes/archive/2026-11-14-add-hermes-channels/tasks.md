# Tasks

## 1. Segredos dos canais

- [x] 1.1 Ferramentas `telegram`, `discord`, `slack` e `slack-app`, domínios e nomes conhecidos, `slack`/`slack-app` pelo prefixo do valor, falso no formato do canal, variáveis e o `.env` do hermes com os falsos dos canais; verificar com testes de unidade e um teste de CLI do `vault add`

## 2. Troca no tráfego

- [x] 2.1 Troca do falso no caminho da URL, com o registro guardando o falso; verificar com teste de unidade e um teste de integração com um servidor HTTPS local no papel do `api.telegram.org`
- [x] 2.2 Upgrade para WebSocket no proxy decifrado, sem `Sec-WebSocket-Extensions`, com a troca nas mensagens de texto do sandbox, quadros reemitidos e a linha `101` no registro; verificar com testes de unidade do leitor de quadros e um teste de integração com um servidor WebSocket local no papel do `gateway.discord.gg`

## 3. Sessão do gateway

- [x] 3.1 `--gateway` (protocolo 19), `hermes gateway run`, recusa noutro harness e de um segundo gateway, marca em `sandmux ls`; verificar com testes de CLI usando um hermes falso
- [x] 3.2 Reinício com espera crescente, `kill` sem relançar, e "aguardando cofre" na restauração com o cofre trancado; verificar com testes do daemon
- [x] 3.3 Aviso do gateway do host no `run --gateway` e no `doctor`; verificar com teste de unidade

## 4. Importação, documentação e verificação

- [x] 4.1 A importação continua deixando `pairing` e `cron` de fora, listados como ignorados; verificar com o teste existente
- [x] 4.2 Documentação: `docs/hermes.md` (nova), `docs/vault.md`, `docs/network.md`, `docs/sessions.md` e o README; verificar com o teste de documentação
- [x] 4.3 Rodar `./scripts/check.sh` sem falhas, sob o vigia de memória
- [ ] 4.4 Verificação à mão com um bot real do Telegram do usuário: o token posto por ele no cofre, o gateway na sessão própria, uma mensagem respondida, e o token real em nenhum arquivo do sandbox
