# Tasks

## 1. Portas do sandbox

- [x] 1.1 Ler as portas em escuta de `/proc/<pid>/net/tcp` e `tcp6` (estado, endereço e porta); verificar com testes de unidade do parser com linhas reais
- [x] 1.2 Criar o auxiliar de conexão (fork antes do gateway, `setns` no namespace de usuário e no de rede, pedido por `socketpair`, descritor por SCM_RIGHTS) e a publicação no supervisor (ouvinte no `127.0.0.1` do host com o mesmo número ou o próximo livre, cópia nos dois sentidos, fechamento, arquivo de portas); verificar com testes de integração pelo `run`: servidor em `0.0.0.0`, porta ocupada no host, porta fechada e servidor só em outro endereço não publicado

## 2. Docker pelo gateway

- [x] 2.1 Trocar o `rootlesskit` para `--net=none`, criar o TUN na visão do Docker e mandar o descritor ao daemon, com o `resolv.conf` para `10.0.2.3`; tirar a exigência de driver de rede do suporte e do `doctor`; verificar com teste de unidade dos argumentos e com o teste de Docker existente
- [x] 2.2 Subir o gateway do Docker no daemon (etiqueta `docker-<id>`) com a política mais restrita das sessões da pasta, recompilada quando sessões sobem ou terminam e quando a configuração muda; verificar com teste de unidade da combinação de políticas e teste de Docker de um contêiner registrado e bloqueado no modo só inferência
- [x] 2.3 Subir o espelho do Docker Hub quando há `registry` ou `docker-registry`, passar `--registry-mirror` ao `dockerd`, decidir a conexão ao espelho como `registry-1.docker.io` e mostrar o estado no `doctor`; verificar com testes de unidade da configuração e dos nomes fixos, e teste de CLI do `doctor`

## 3. Portas no daemon e na TUI

- [x] 3.1 Acrescentar `PortInfo` e `ports` ao `StatsOf` (protocolo 9), com as portas do arquivo da sessão e as dos contêineres pela API do Docker, e mostrar no `sandmux stats`; verificar com teste de PTY
- [x] 3.2 Acrescentar a seção Portas ao painel; verificar com teste de PTY

## 4. Documentação e verificação

- [x] 4.1 Atualizar `docs/network.md`, `docs/docker.md`, `docs/tui.md`, `docs/sessions.md` e o README; verificar com o teste de documentação
- [x] 4.2 Rodar `./scripts/check.sh` sem falhas
- [x] 4.3 Verificar à mão uma sessão real com um servidor de desenvolvimento aberto no navegador do host, um contêiner com acesso registrado pelo gateway, e o cache entre duas pastas com o `registry` instalado (se o usuário instalar o pacote)
