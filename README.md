# sandmux

Multiplexador de terminal para rodar agentes de código dentro de um sandbox de segurança.

O sandmux sobe harnesses de agente (opencode, hermes) em abas, no estilo do tmux. Cada aba roda confinada: o agente enxerga a pasta de trabalho e mais nada do seu HOME, o tráfego de rede é registrado e pode ser bloqueado, e nenhuma credencial real entra no sandbox.

> **Situação: primeira etapa em implementação.** O confinamento por linha de comando (`sandmux run`, `import` e `doctor`) existe e tem testes. Daemon, abas, controle de rede e cofre de segredos ainda não existem. A seção "O que já funciona" descreve esta etapa; o resto do documento descreve o que foi decidido para a v1.

## Por quê

Um harness de agente roda hoje com o mesmo acesso que você: lê `~/.ssh`, credenciais de cloud, tokens de outros programas e qualquer pasta do disco. Um agente manipulado por prompt injection pode ler esses segredos e enviá-los para fora.

O sandmux parte de um princípio: **seguro por padrão, com o mínimo de atrito**. Uma baseline segura vem embutida e funciona sem configurar nada. Você só gerencia as exceções.

## Como funciona

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

## O que já funciona

### Instalação a partir do código

Precisa de Rust 1.88 ou mais novo.

```bash
cargo build --release
```

```bash
install -m 755 target/release/sandmux target/release/sandmux-run ~/.local/bin/
```

```bash
sandmux doctor
```

O `doctor` confere o kernel (Landlock ABI 6 ou mais novo), se user namespaces sem privilégio permitem montar e se a camada de escrita funciona. No Ubuntu, que restringe user namespaces por AppArmor por padrão, ele mostra o perfil do lançador e o comando com `sudo` que o instala. O perfil vale para o caminho do `sandmux-run`: se o binário mudar de lugar, rode o `doctor` de novo e reinstale.

### Comandos

| Comando | O que faz |
|---|---|
| `sandmux run <harness>` | Sobe o harness confinado na pasta atual. Harnesses: `opencode`, `hermes` e `shell` (um shell confinado, sem agente). Na primeira execução, pergunta se deve importar a configuração que você já tem. Num terminal, cria uma sessão no daemon e anexa a ela; sem terminal (script, pipe, CI), roda direto, em primeiro plano, sem daemon |
| `sandmux ls` | Lista as sessões: identificador, nome, estado (viva, encerrada com o código, parada com o motivo), terminais anexados e pasta |
| `sandmux attach [id]` | Anexa este terminal a uma sessão. Sem identificador, à única sessão viva |
| `sandmux detach <id>` | Desanexa todos os terminais de uma sessão, sem encerrá-la |
| `sandmux kill <id>` | Encerra a sessão e a tira da lista |
| `sandmux restart <id>` | Relança uma sessão encerrada ou parada, na mesma pasta e com as mesmas opções |
| `sandmux daemon status`, `stop`, `restart`, `disable` | Estado do daemon; parar (as sessões vivas voltam na próxima subida); reiniciar com o binário instalado; parar e remover a unidade systemd |
| `sandmux import <harness>` | Copia a configuração existente do harness para o HOME sintético, trocando os segredos por valores falsos. Rodado de novo, copia só o que é novo; `--overwrite` substitui |
| `sandmux doctor` | Mostra se a máquina tem o suporte necessário e o que falta |

Opções de `run`:

| Opção | Efeito |
|---|---|
| `--workdir <pasta>` | Usa outra pasta de trabalho no lugar da pasta atual |
| `--ro <caminho>`, `--rw <caminho>` | Acrescenta um caminho do host em somente leitura ou com escrita. Montar uma credencial conhecida (`~/.kube`, `~/.aws`) gera aviso |
| `--pass-env <NOME>` | Passa uma variável do seu ambiente para dentro, com aviso. É o jeito de entregar a chave do modelo enquanto o cofre não existe |
| `--import`, `--no-import` | Responde à pergunta da primeira execução sem terminal interativo |
| `--repo` | Numa subpasta de repositório, monta o repositório inteiro em somente leitura para o git funcionar |
| `--kvm` | Libera `/dev/kvm` |
| `--no-gpu` | Não expõe a GPU (exposta por padrão quando existe) |
| `--allow-clipboard` | Deixa o harness gravar no clipboard do seu terminal (OSC 52). Por padrão a gravação é bloqueada, e a leitura do clipboard é sempre bloqueada |
| `-- <argumentos>` | Argumentos repassados ao harness |

Exemplos:

```bash
sandmux run shell -- -c "id -u; ls ~"
```

```bash
sandmux run opencode --pass-env OPENROUTER_API_KEY
```

### Sessões

Num terminal, `sandmux run` cria uma sessão no daemon de sessões (`sandmuxd`) e anexa o terminal a ela. O agente continua rodando quando o terminal fecha, e `sandmux attach` volta a ele de qualquer terminal, com o histórico e a tela atual. Enquanto anexado:

| Teclas | Efeito |
|---|---|
| Ctrl+b d | Desanexa; a sessão continua |
| Ctrl+b Ctrl+b | Envia um Ctrl+b ao harness |

Vários terminais podem se anexar à mesma sessão, e todos digitam. A tela segue o menor deles. O histórico (10.000 linhas por sessão) fica só em memória e some quando o daemon para.

Na primeira vez em que precisa do daemon, o cliente instala e habilita a unidade systemd de usuário `sandmuxd.service`, que sobe o daemon no login, e avisa. Sem systemd, o cliente sobe o daemon na hora. Depois de um reboot, as sessões que estavam vivas voltam sozinhas, menos as que usavam `--pass-env`: essas ficam paradas até um `sandmux restart <id>` num terminal com as variáveis. Detalhes em [docs/sessions.md](docs/sessions.md).

Para entregar ao agente um arquivo de fora da pasta de trabalho (um PDF, um screenshot), copie-o para a pasta de entrada que o `doctor` mostra. Lá dentro ela aparece em `~/inbox`, em somente leitura.

Os dados do sandmux (HOMEs sintéticos, camadas de escrita, pasta de entrada, manifestos de importação) ficam em `~/.local/share/sandmux`. Para apagar uma camada de escrita, libere a permissão antes, porque o overlay cria pastas sem permissão: `chmod -R u+rwX ~/.local/share/sandmux/layers && rm -rf ~/.local/share/sandmux/layers/<harness>`.

### Limites desta etapa

- A rede do host fica aberta e sem registro.
- Arquivos de segredo que já estão na pasta de trabalho (`.env`, chaves) ficam legíveis pelo agente. Tire-os da pasta antes.
- Uma variável passada com `--pass-env` fica ao alcance do agente.
- No hermes, o `--pass-env` coloca o valor real no `~/.hermes/.env` só daquela execução, e o arquivo fica em somente leitura: o hermes não consegue salvar configurações no `.env` enquanto roda assim. O `.env` guardado continua com os valores falsos. Valores com quebra de linha ou `${` são recusados.
- O agente não roda contêineres, não instala pacotes do sistema e não abre programas com janela.
- A proteção do `.git` tem limites conhecidos, descritos em [docs/git.md](docs/git.md).

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
- Kernel 6.12 ou mais novo na primeira etapa (Landlock com escopo de sockets abstratos), com user namespaces sem privilégio. O overlayfs em user namespace é opcional: sem ele, os caches começam vazios e as toolchains ficam em somente leitura.
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
| [docs/launch-spec.md](docs/launch-spec.md) | Formato da especificação de lançamento entre o cliente e o lançador |
| [docs/git.md](docs/git.md) | O que a proteção do `.git` cobre e como o git se comporta dentro do sandbox |
| [docs/sessions.md](docs/sessions.md) | O daemon de sessões: socket, protocolo, lista de sessões, restauração e limites |
| [openspec/roadmap.md](openspec/roadmap.md) | Todas as decisões do projeto e o plano em 10 etapas |
| [openspec/specs/](openspec/specs/) | Specs vigentes: isolamento, sistema de arquivos, presets dos harnesses e importação de estado |
| [openspec/changes/archive/2026-11-13-add-sandbox-cli/proposal.md](openspec/changes/archive/2026-11-13-add-sandbox-cli/proposal.md) | Proposta do primeiro change: o confinamento por linha de comando |
| [openspec/changes/archive/2026-11-13-add-sandbox-cli/design.md](openspec/changes/archive/2026-11-13-add-sandbox-cli/design.md) | Desenho técnico do primeiro change |
| [AGENTS.md](AGENTS.md) | Regras para agentes que trabalham neste repositório |

O primeiro change (`add-sandbox-cli`) está implementado e arquivado. Para rodar a verificação completa (formatação, lints e testes, inclusive os testes de fuga do sandbox): `./scripts/check.sh`.

## Licença

Licença dupla, MIT ou Apache-2.0, à sua escolha: [LICENSE-MIT](LICENSE-MIT) e [LICENSE-APACHE](LICENSE-APACHE).
