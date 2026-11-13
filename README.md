# sandmux

Multiplexador de terminal para rodar agentes de código dentro de um sandbox de segurança.

O sandmux sobe harnesses de agente (opencode, hermes) em abas, no estilo do tmux. Cada aba roda confinada: o agente enxerga a pasta de trabalho e mais nada do seu HOME, o tráfego de rede é registrado e pode ser bloqueado, e nenhuma credencial real entra no sandbox.

> **Situação: em planejamento.** Ainda não existe código. Este repositório guarda as decisões de projeto e o plano de desenvolvimento. Os comandos e comportamentos descritos abaixo são o que foi decidido, não o que já funciona.

## Por quê

Um harness de agente roda hoje com o mesmo acesso que você: lê `~/.ssh`, credenciais de cloud, tokens de outros programas e qualquer pasta do disco. Um agente manipulado por prompt injection pode ler esses segredos e enviá-los para fora.

O sandmux parte de um princípio: **seguro por padrão, com o mínimo de atrito**. Uma baseline segura vem embutida e funciona sem configurar nada. Você só gerencia as exceções.

## Como vai funcionar

Você entra na pasta do projeto e sobe o agente:

```bash
cd ~/projetos/app
sandmux run opencode
```

A pasta de onde o comando foi executado é montada dentro do sandbox no mesmo caminho, com leitura e escrita. É a pasta real, não uma cópia: o código que o agente cria aparece no host na hora, e você pode abri-lo no editor enquanto ele trabalha.

```
host                                  sandbox
~/projetos/app/   <-- mesma pasta --> ~/projetos/app/    leitura e escrita
~/ (resto)                            ~/ (HOME sintetico, separado)
/usr, /etc, ...   ------------------> somente leitura
```

O que o agente enxerga:

| Recurso | Dentro do sandbox |
|---|---|
| Pasta de trabalho | A pasta real, com leitura e escrita. `.git/hooks` e `.git/config` ficam em somente leitura |
| HOME | Um HOME sintético por harness. O HOME real não existe lá dentro |
| Sistema (`/usr`, `/etc`) | Somente leitura, com os arquivos sensíveis cobertos |
| Toolchains (cargo, nvm, uv, go e outras) | Lidas do host, escritas numa camada privada. `cargo install` e `npm install -g` funcionam sem alterar o host |
| Sockets do host (ssh-agent, D-Bus, Docker, Wayland, X11) | Fora do alcance |
| Processos do host | Invisíveis |
| Variáveis de ambiente | Só uma lista segura; o resto não é herdado |

Rodar o comando direto no HOME, na raiz ou numa pasta que contém o HOME é recusado. Tem que ser uma subpasta.

### Comandos previstos na primeira etapa

| Comando | O que faz |
|---|---|
| `sandmux run <harness>` | Sobe o harness confinado na pasta atual. Harnesses: `opencode`, `hermes` e `shell` (um shell confinado, sem agente) |
| `sandmux import <harness>` | Copia a configuração existente do harness para o HOME sintético, trocando os segredos por valores falsos |
| `sandmux doctor` | Mostra se a máquina tem o suporte necessário e o que falta |

Opções de `run`: `--ro` e `--rw` acrescentam pastas à visão do agente, e `--pass-env` passa uma variável de ambiente específica, com aviso.

## O que está planejado para a v1

- **Confinamento**: user, mount, pid e network namespaces, Landlock e seccomp, sem precisar de root. Sem o suporte de kernel exigido, o agente não sobe; não existe aba sem sandbox.
- **Sessões persistentes**: um daemon mantém os agentes vivos quando o terminal fecha.
- **Interface**: abas com terminal confinado, painel lateral com o registro de acessos, tecla de prefixo (Ctrl+b por padrão), mouse e temas.
- **Rede**: todo o tráfego passa por um gateway que registra cada domínio. Quatro modos por aba: aberta (padrão), só inferência, sem rede e custom (bloqueado por padrão, com allowlist).
- **Segredos fora do sandbox**: um cofre cifrado guarda as chaves reais. O agente recebe uma chave falsa, trocada pela real no tráfego de saída, só quando o destino é o dono da chave. Vale para a API de inferência, para o `gh` e para registries privados.
- **Arquivos**: registro de todo acesso a arquivo, allowlist e denylist com efeito imediato, e arquivos com cara de segredo escondidos do agente.
- **Docker**: um Docker sem root, privado do sandbox. O socket do Docker do host nunca entra.
- **MCPs**: controle de quais servidores MCP estão liberados, e um MCP embutido para PostgreSQL e MySQL com a credencial injetada fora do sandbox.
- **Assistente**: um modal de chat e um modal que gera um comando shell e o escreve no terminal, sem executar.

A ordem de desenvolvimento, as decisões e os riscos aceitos estão no [roadmap](openspec/roadmap.md).

## Limites conhecidos

- **Ameaça alvo**: um agente manipulado que tenta ler segredos e exfiltrar dados. Exploit de kernel está fora do escopo.
- **A pasta de trabalho é do agente.** Ele lê tudo que está nela. O que ele escreve ali (`Makefile`, scripts do `package.json`, `.envrc`) precisa ser lido antes de ser executado no host.
- **Nas primeiras etapas a proteção é parcial.** Até o controle de rede existir, a rede do host fica aberta e sem registro. Até a política de arquivos existir, um `.env` que já está na pasta de trabalho fica legível. Tire os segredos da pasta antes de usar.
- **Sem programas com janela na v1.** Navegador headless funciona.

## Requisitos

- Linux. Não há suporte a macOS nem a Windows.
- Kernel 6.12 ou mais novo na primeira etapa (Landlock com escopo de sockets abstratos), com user namespaces sem privilégio e overlayfs em user namespace.
- Distros que a v1 vai testar e suportar: Ubuntu 26.04, Debian 13, Fedora atual e Arch.
- Os harnesses (opencode, hermes) são instalados por você, no host. O sandmux não instala nem atualiza harness.

## Arquitetura

Quatro binários, cada um com uma fronteira de confiança:

```
+----------------+  socket  +----------------------+  socket  +----------------+
| sandmux        |<-------->| sandmuxd             |<-------->| sandmux-helper |
| cliente        |          | daemon de sessoes    |          | auxiliar root  |
| (CLI e TUI)    |          | PTYs, gateway, cofre |          | eBPF           |
+----------------+          | registro, politica   |          | (pos-v1)       |
                            +----------+-----------+          +----------------+
                                       |
                                       | especificacao de lancamento
                                       v
                            +----------------------+
                            | sandmux-run          |
                            | lancador do sandbox  |
                            +----------+-----------+
                                       |
                                       v
                                    harness
```

- `sandmux`: cliente de linha de comando e interface de terminal. Não toca em segredo nem em sandbox.
- `sandmuxd`: daemon de sessões. Hospeda os terminais, o gateway de rede, o cofre, o registro e a política.
- `sandmux-run`: lançador do sandbox. Só aplica uma especificação pronta; não conhece harness nem política.
- `sandmux-helper`: auxiliar root para eBPF, previsto para depois da v1.

Escrito em Rust, com ratatui na interface.

## Desenvolvimento

O projeto usa [OpenSpec](https://github.com/Fission-AI/OpenSpec) para planejar cada mudança antes de implementar.

| Arquivo | Conteúdo |
|---|---|
| [openspec/roadmap.md](openspec/roadmap.md) | Todas as decisões do projeto e o plano em 10 etapas |
| [openspec/changes/add-sandbox-cli/proposal.md](openspec/changes/add-sandbox-cli/proposal.md) | Proposta do primeiro change: o confinamento por linha de comando |
| [openspec/changes/add-sandbox-cli/design.md](openspec/changes/add-sandbox-cli/design.md) | Desenho técnico do primeiro change |
| [AGENTS.md](AGENTS.md) | Regras para agentes que trabalham neste repositório |

O primeiro change tem proposta e desenho; faltam as specs e as tarefas. Nada foi implementado nem testado.

## Licença

A licença decidida é dupla, MIT e Apache-2.0. Os arquivos de licença ainda não foram adicionados ao repositório.
