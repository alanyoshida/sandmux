# Proposal

## Why

Harnesses de agente (opencode, hermes) rodam hoje com o mesmo acesso do usuário: leem `~/.ssh`, credenciais de cloud, tokens de outros harnesses e qualquer pasta do disco. O sandmux existe para rodar esses agentes confinados, seguros por padrão e sem exigir configuração. Esta é a primeira fatia do projeto e ataca a parte de maior incerteza: provar que os dois harnesses funcionam de verdade dentro de um sandbox montado sem privilégio, antes de existir daemon, TUI ou controle de rede.

## What Changes

- Novo binário `sandmux-run`, o lançador do sandbox: recebe uma especificação de lançamento já resolvida, monta o isolamento (user, mount, pid, ipc e uts namespaces, Landlock, seccomp) e executa o harness. Não conhece harnesses nem política; só aplica o que recebe.
- Novo binário `sandmux`, nesta fatia apenas como linha de comando (a TUI chega em outro change), com três comandos:
  - `sandmux run <harness>`: compõe a baseline embutida, o preset do harness e as opções da linha de comando, e chama o lançador. Na primeira execução de um harness, pergunta se deve importar a configuração existente.
  - `sandmux import <harness>`: copia o estado existente do harness para o HOME sintético, trocando segredos por valores falsos. Rodado de novo, copia só o que é novo e nunca sobrescreve.
  - `sandmux doctor`: informa se a máquina tem o suporte necessário e o que falta.
- Baseline de segurança embutida, aplicada sem nenhuma configuração:
  - a pasta de trabalho é, por padrão, a pasta de onde o comando foi executado: a pasta real do host, montada no mesmo caminho com leitura e escrita. O que o agente cria nela aparece no host na hora;
  - o agente enxerga apenas a pasta de trabalho, um HOME sintético e o sistema em somente leitura;
  - o HOME real, sockets do host (ssh-agent, D-Bus, Docker, X11/Wayland) e processos do host ficam fora do alcance;
  - `/etc/passwd` e `/etc/group` são substituídos por versões geradas; arquivos sensíveis de `/etc` ficam inacessíveis;
  - variáveis de ambiente não são herdadas, exceto uma lista segura;
  - o `/dev` é mínimo, mais a GPU quando existe; `/dev/kvm` só por opção explícita;
  - uma pasta de trabalho perigosa (o HOME, a raiz ou uma pasta que contém o HOME) é sempre recusada.
- Toolchains do HOME real entram a partir de uma lista conhecida (nvm, bun, cargo, rustup, uv, pyenv, go, linuxbrew), com arquivos de credencial conhecidos excluídos.
- Toolchains e caches de pacotes são lidos do host e escritos numa camada privada do sandbox. Instalações globais (`cargo install`, `npm install -g`, `rustup component add`, `go install`) funcionam lá dentro e nunca alteram o host.
- Proteção do `.git` da pasta de trabalho: `.git/hooks` e `.git/config` ficam em somente leitura. O agente faz commit, branch e merge, mas não consegue plantar um hook nem alterar a configuração que o git do host vai obedecer. Worktrees do git funcionam; numa subpasta de repositório, o git só funciona se o usuário pedir para montar o repositório inteiro em somente leitura.
- Montar de propósito uma credencial conhecida (por exemplo `--ro ~/.kube`, para CLIs de cloud) é permitido e gera aviso de que ela fica ao alcance do agente.
- Um HOME sintético por harness, compartilhado entre execuções, com um gitconfig mínimo gerado só com o nome e o e-mail do usuário.
- Uma pasta de entrada do sandmux, visível em somente leitura em todo sandbox, para entregar ao agente um arquivo de fora da pasta de trabalho (screenshot, PDF).
- Presets embutidos para opencode e hermes (onde está o binário, o que montar, quais variáveis definir). O backend Docker do hermes não é suportado. Quando o harness não está instalado na máquina, o `run` recusa e mostra como instalar; o sandmux não instala harness.
- Preset `shell`: um shell confinado, sem harness, lançado como os outros (`sandmux run shell`), com HOME sintético próprio e um arquivo de configuração mínimo gerado. O `.bashrc` e o `.zshrc` reais não são importados.
- O código do harness entra em somente leitura e o preset desliga a checagem de atualização. Atualizar o harness é feito fora do sandbox, pelo usuário.
- Falha fechada: sem o suporte de kernel exigido, o agente não sobe.
- Passagem explícita de variável de ambiente (`--pass-env`), desligada por padrão e com aviso visível, para o harness conseguir chamar o modelo enquanto o cofre não existe.

Fora deste change, cada item em um change posterior:

- Isolamento e controle de rede. Nesta fatia o sandbox usa a rede do host, sem filtro e sem log.
- Cofre de segredos e troca de chave no tráfego, inclusive os tokens do `gh` e de registries privados.
- Docker para o agente. O socket do Docker do host nunca entra no sandbox; um Docker sem root, privado do sandbox, chega com o daemon. Nesta fatia o agente não roda contêineres.
- Tratamento de arquivos com cara de segredo na pasta de trabalho, log de acesso a arquivos e mudança de política com o agente em execução. Nesta fatia nada é escondido: um `.env` que já está na pasta de trabalho fica legível pelo agente.
- Programas com janela. Wayland e X11 ficam fora do sandbox; navegador headless funciona.
- Daemon de sessões, TUI, abas e arquivo de configuração em TOML com perfis.
- Limites de recurso por aba.
- Instalador com `sudo` (perfil AppArmor) e tudo que envolve eBPF.

## Capabilities

### New Capabilities

- `sandbox-isolation`: como um processo é confinado ao ser lançado (camadas de isolamento, ordem de aplicação, falha fechada, ambiente, ciclo de vida do processo e diagnóstico de suporte da máquina).
- `sandbox-filesystem`: o que o agente enxerga do disco (pasta de trabalho com `.git/hooks` e `.git/config` protegidos, HOME sintético com gitconfig mínimo, sistema em somente leitura, substituições em `/etc`, toolchains conhecidas, camada privada de escrita para toolchains e caches, e pasta de entrada em somente leitura).
- `harness-presets`: o que cada harness suportado precisa para rodar confinado (localização, montagens, variáveis de ambiente, checagem de atualização desligada), começando por opencode, hermes e o preset `shell`.
- `harness-state-import`: importação do estado existente de um harness para o HOME sintético, com troca de segredos por valores falsos e exclusão de histórico, oferecida na primeira execução do harness; a reimportação copia só o que é novo.

### Modified Capabilities

Nenhuma. O projeto ainda não tem specs.

## Impact

- **Código**: projeto novo. Cria o workspace Rust com dois binários (`sandmux-run`, `sandmux`) e bibliotecas compartilhadas para a especificação de lançamento, o sandbox e os presets.
- **Plataforma**: apenas Linux. Exige user namespaces sem privilégio, overlayfs em user namespace e Landlock com escopo de sockets abstratos (kernel 6.12 ou mais novo). O requisito de Landlock pode baixar quando o isolamento de rede chegar.
- **Dependências**: crates de Landlock, de filtro seccomp e de chamadas de sistema; sem dependência de bibliotecas C além da libc.
- **Dados do usuário**: cria um diretório de dados do sandmux com os HOMEs sintéticos e as camadas de escrita de toolchains e caches. O HOME real nunca é alterado; a importação é uma cópia de mão única.
- **Limitação conhecida desta fatia**: a rede do host fica aberta e sem registro, uma variável passada com `--pass-env` fica ao alcance do agente, arquivos de segredo que já estão na pasta de trabalho ficam legíveis, e o agente não roda contêineres, não instala pacotes do sistema e não abre programas com janela. O resultado serve para validar o confinamento e para uso consciente, não como a proteção completa do projeto.
