---
title: sandmux
description: multiplexador de terminal que roda agentes de código dentro de um sandbox
headline: Cada agente numa aba confinada.
lede: Um multiplexador de terminal, no estilo do tmux, que sobe opencode, Claude Code, hermes e outros agentes dentro de um sandbox no Linux. O agente enxerga a pasta do projeto e mais nada do seu HOME, a rede passa por um gateway que registra e filtra, e nenhuma credencial real entra lá dentro.
install: curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
cta_primary: Instalar e começar

sees_title: O que o agente enxerga
sees_text: Você roda `sandmux run opencode` na pasta do projeto. A pasta é montada no mesmo caminho, com leitura e escrita; o resto do sistema é outro.
sees_col_resource: Recurso
sees_col_inside: Dentro do sandbox
sees:
  - resource: Pasta de trabalho
    inside: A pasta real, com leitura e escrita. `.git/hooks` e `.git/config` ficam em somente leitura.
  - resource: HOME
    inside: Um HOME sintético por harness. O HOME real não existe lá dentro.
  - resource: Sistema (`/usr`, `/etc`)
    inside: Somente leitura, com os arquivos sensíveis cobertos.
  - resource: Toolchains (cargo, nvm, uv, go)
    inside: Lidas do host e escritas numa camada privada. `cargo install` e `npm install -g` funcionam sem alterar o host.
  - resource: Arquivos com cara de segredo
    inside: "`.env`, `*.pem` e `id_rsa` da pasta não abrem, a menos que o próprio agente os tenha criado."
  - resource: Sockets do host
    inside: ssh-agent, D-Bus, Docker, Wayland e X11 fora do alcance.
  - resource: Chaves de API e tokens
    inside: Só valores falsos. O gateway troca pelo real no tráfego, e só quando o destino é o dono da chave.

gallery_title: A interface
gallery_text: Abas como no tmux, com Ctrl+b. O painel lateral mostra a proteção aplicada, a memória e cada conexão da aba, com o motivo de cada bloqueio. As telas abaixo são gravações do sandmux real.

layers_title: Camadas do kernel, sem VM nem contêiner
layers_text: O sandbox usa o que o Linux já oferece, sem root no uso normal. Sem Landlock e user namespaces, o agente não sobe; não existe aba sem sandbox.
layers_sys: Chamadas de sistema
layers_net: Tráfego de rede
layers_agent: agente
layers_agent_does: opencode, Claude Code, hermes, oh-my-pi, Antigravity ou um shell
layers:
  - name: seccomp
    does: Nega montagem, `bpf`, módulos, `kexec`, `io_uring` e afins, e manda cada abertura de arquivo ao supervisor.
  - name: Landlock
    does: O que pode ser lido, escrito e executado; nada de sockets abstratos, sinais ou `ioctl` para fora.
  - name: mount namespace
    does: O que existe para o agente. HOME sintético, `/etc` coberto, `/dev` mínimo.
  - name: user e pid namespaces
    does: O seu UID, sem capabilities, vendo só os processos do sandbox.
  - name: cgroup
    does: Teto de memória e de processos; passou do teto, o kernel mata só aquele sandbox.
net:
  - name: network namespace
    does: Só um TUN, sem rota direta para fora.
  - name: gateway
    does: DNS, política de domínios, decifração só onde há segredo, troca de chave e registro.
  - name: internet e rede local
    does: Metadados de cloud, QUIC e ICMP bloqueados em todo modo.
layers_more: Ler a arquitetura

security_title: Feito para quem precisa decidir se confia
security_text: A ameaça é um agente manipulado por prompt injection, tentando ler segredos e mandar dados para fora. O modelo de ameaça, o que cada camada impede e os limites estão escritos, com links para as specs.
stops_title: O que o sandbox impede
stops:
  - Ler `~/.ssh`, `~/.aws`, `~/.config/gh` e o resto do HOME
  - Roubar a chave de API do provedor
  - Mandar dados para destinos de exfiltração conhecidos
  - Plantar um hook ou um remote no `.git`
  - Falar com o ssh-agent, o D-Bus ou o Docker do host
  - Ver ou sinalizar processos do host
limits_title: O que ele não promete
limits:
  - Resistir a um exploit de kernel
  - Proteger o que você libera de propósito, como `--rw` ou `--pass-env`
  - "No modo `aberta`, o padrão: impedir o envio do código a um domínio fora da denylist; ele registra e destaca"
  - Impedir o uso da chave falsa contra o próprio dono, como gastar créditos de inferência
security_more: Ler o modelo de ameaça e os limites

install_title: Instalação
install_text: Binários estáticos para Linux `x86_64`, kernel 6.12 ou mais novo. O script pede `sudo`; sem ele, instala em `~/.local/bin`. Também há `.deb`, `.rpm`, AUR, Homebrew e `cargo install`.
install_steps:
  - curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
  - sandmux doctor
  - "# na pasta do projeto"
  - cd ~/projetos/app
  - sandmux run opencode
install_more: Guia de instalação, com o Ubuntu 24.04
---
