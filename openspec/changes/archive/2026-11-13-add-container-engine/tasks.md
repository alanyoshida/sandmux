# Tasks

## 1. Visão do Docker e modo do lançador

- [x] 1.1 Implementar `compose_docker_view` no `sandmux-policy`, reaproveitando a composição do sandbox:
  - sistema em somente leitura com as coberturas de `/etc`;
  - pasta de trabalho com a proteção do `.git`;
  - pasta de entrada;
  - `/dev` e `/sys` do host, `/proc` novo, `/tmp` e `/run` em tmpfs;
  - as pastas do Docker com escrita.

  Verificar com testes de unidade de que a visão contém essas montagens e não contém o HOME, os caches nem as toolchains
- [x] 1.2 Implementar no `sandmux-run` o modo `--view-file <arquivo>` (montar a raiz da especificação, `pivot_root`, executar o comando), sem namespaces novos, Landlock nem seccomp; verificar com teste de integração dentro de `unshare --user --map-root-user --mount --pid --fork` que o comando enxerga só a visão
- [x] 1.3 Implementar a detecção de suporte do Docker privado (binários, faixas em `/etc/subuid` e `/etc/subgid` por nome ou UID, driver de rede); verificar com testes de unidade da leitura das faixas e da escolha do driver

## 2. Docker por pasta no daemon

- [x] 2.1 Implementar o gerente do Docker por pasta (identificador pelo hash FNV-1a da pasta, pastas de dados e de estado, subida por `systemd-run` com `Delegate=yes` e tetos da pasta quando há suporte, `rootlesskit` e `sandmux-run --view-file`, espera do socket, log, parada com SIGTERM e SIGKILL e parada do escopo); verificar com teste de integração que sobe o Docker de uma pasta, roda `docker info` pelo socket e, ao parar, não deixa processo
- [x] 2.2 Criar o socket de cada sessão, montá-lo em `/run/sandmux/docker.sock` com `DOCKER_HOST` na composição, e repassar as conexões ao Docker da pasta (subida na primeira conexão, cópia de bytes nos dois sentidos); verificar com teste de PTY que `docker info` dentro de uma sessão funciona e que uma sessão sem `docker` não sobe o Docker
- [x] 2.3 Responder `503` com a mensagem de erro quando não há suporte ou a subida falha; verificar com teste de PTY sem `rootlesskit` no `PATH` que `docker ps` mostra o erro com `sandmux doctor`
- [x] 2.4 Parar o Docker da pasta quando a última sessão viva dela termina e apagar o socket da sessão; parar todos no `daemon stop`; verificar com testes de que nenhum processo de `rootlesskit` ou `dockerd` daquela pasta sobra
- [x] 2.5 Não montar socket nem definir `DOCKER_HOST` no `run` sem terminal; verificar com teste de integração

## 3. Comportamento do Docker privado

- [x] 3.1 Verificar com testes de PTY usando uma imagem `FROM scratch` com o `busybox` estático do host, pulando sem suporte:
  - `-v $PWD:/app` lê e grava na pasta de trabalho;
  - `-v ~/.ssh` e um link plantado para `~/.ssh` chegam vazios;
  - gravar em `.git/hooks` pela montagem falha;
  - a imagem persiste para a sessão seguinte na mesma pasta e não aparece noutra pasta.
- [x] 3.2 Verificar com teste de PTY que `-p 18080:80` com o `httpd` do `busybox` responde em `127.0.0.1` e não escuta em `0.0.0.0`
- [x] 3.3 Verificar com teste de PTY que `docker compose up -d` com um `compose.yaml` de imagem local sobe o serviço e `docker compose ps` o mostra
- [x] 3.4 Verificar com teste de PTY que, com `memory = "512M"` no perfil da pasta, um contêiner que aloca 2 GB é morto e o Docker continua respondendo

## 4. Medição, diagnóstico e documentação

- [x] 4.1 Acrescentar a seção do Docker privado ao `StatsOf` e ao `sandmux stats` (memória, processos, tetos, contêineres rodando), subir `PROTOCOL` para 4; verificar com teste de PTY com dois contêineres rodando e com os testes do `sandmux-proto`
- [x] 4.2 Acrescentar ao `doctor` a verificação do Docker privado (o que falta, o driver de rede e o aviso de driver experimental); verificar com teste de CLI com e sem `rootlesskit` no `PATH`
- [x] 4.3 Escrever `docs/docker.md`, com:
  - a visão de arquivos e o que não entra;
  - o socket, a subida e a parada;
  - a persistência e como apagar os dados;
  - as portas e a rede;
  - os tetos e os limites.

  Atualizar o README e o `docs/limits.md`. Verificar com o teste de documentação

## 5. Verificação final

- [x] 5.1 Rodar `./scripts/check.sh` sem falhas, sem daemon, `dockerd` ou `rootlesskit` de teste sobrando
- [x] 5.2 Verificar à mão numa sessão real do shell: construir e rodar uma imagem, usar `docker compose`, conferir no `sandmux stats` o Docker privado e, ao encerrar a sessão, que o Docker parou
