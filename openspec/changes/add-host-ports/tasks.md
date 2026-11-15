## 1. Rede

- [x] 1.1 Auxiliar de conexão com a operação `L` (ouvinte no `127.0.0.1` do sandbox) e o cliente `Connector::listen`
- [x] 1.2 `NetPolicy.host_ports`; `hostports::run` com o laço, o repasse, o registro e a denylist; exclusão das portas do host na publicação de entrada

## 2. Configuração e importação

- [x] 2.1 Chave `host-ports` (camadas, edição, política efetiva, chat), `--host-port` e `LaunchOptions.host_ports`; protocolo 22
- [x] 2.2 Aviso da importação para URLs locais com porta

## 3. Verificação e documentação

- [x] 3.1 Testes: porta do host alcançada pelo `127.0.0.1` do sandbox, recusa sem a chave, linha no registro, denylist, porta acrescentada ao vivo, sem eco na publicação, aviso da importação
- [x] 3.2 `docs/network.md`, `docs/config.md`, README e teste de documentação
- [ ] 3.3 Verificação à mão pelo usuário: o opencode com o LM Studio do host em `127.0.0.1:8888` e `host-ports = [8888, 9000]`
