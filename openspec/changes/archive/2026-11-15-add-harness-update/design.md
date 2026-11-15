## Context

Cada preset monta o código do harness do host em somente leitura e desliga a atualização automática dentro do sandbox (`OPENCODE_DISABLE_AUTOUPDATE`, `updates.check: false`, `DISABLE_AUTOUPDATER=1`). O `omp` não tem uma chave equivalente conhecida, e a pasta do binário não é gravável lá dentro.

Os quatro harnesses têm um atualizador oficial, confirmado nos binários instalados: `opencode upgrade`, `hermes update`, `claude update` e `omp update`.

## Goals / Non-Goals

**Goals:**
- Um comando só para atualizar qualquer harness suportado, sem o usuário precisar saber o atualizador de cada um.
- Avisar sobre as abas vivas antes e depois.

**Non-Goals:**
- Instalar um harness ausente.
- Atualizar o próprio sandmux.
- Atualizar pela TUI. O atualizador pode fazer perguntas e mostrar progresso no terminal, e a TUI não tem um terminal fora do sandbox. Fica para depois, se fizer falta.
- Reiniciar as abas automaticamente.

## Decisions

- **Roda no processo da CLI, não no daemon.** O atualizador herda o terminal, o ambiente e o HOME do usuário, como se ele o rodasse à mão. O daemon nunca executa código do harness fora do sandbox.
  - Alternativa: o daemon rodar o atualizador. Recusada: o daemon pode ter outro ambiente (systemd), sem terminal para as perguntas do atualizador.
- **A tabela de atualizadores fica em `sandmux-policy`**, junto dos presets, e usa a mesma checagem de instalação de cada preset. Assim o `update` recusa exatamente quando o `run` recusaria, com a mesma mensagem.
  - Para o `claude`, o executável é o link `~/.local/bin/claude`, não a versão resolvida, porque o `claude update` troca o alvo do link.
- **Abas vivas:** a CLI pergunta ao daemon só se ele já estiver rodando (`connect`, sem `connect_or_start`). Sem daemon, não há abas vivas. Contam as sessões do mesmo harness em execução.
  - A pergunta é necessária porque o `hermes update` troca arquivos Python na pasta que as abas montam ao vivo. No opencode, no claude e no omp, o processo vivo continua com o arquivo antigo, mas a aba só pega a versão nova ao reiniciar.
  - Sem terminal, não há a quem perguntar: recusa, e `--yes` assume a responsabilidade.
- **Argumentos depois de `--`** vão para o atualizador sem tradução, porque cada um tem opções próprias (`--check`, `--canary`, uma versão).
- **Nome `update`**, ao lado do `import <harness>`. O sandmux em si continua sendo atualizado pelo pacote ou pelo `install.sh`, e a ajuda do comando diz isso.

## Risks / Trade-offs

- [O atualizador roda sem confinamento] → É o mesmo código que o usuário rodaria à mão, do mesmo lugar. O sandbox nunca escreve nesses caminhos, então o agente não consegue plantar nada que o atualizador execute.
- [Um atualizador muda de nome ou de comando] → A tabela é por harness e versionada com o binário. Um erro do atualizador chega ao usuário com o código de saída dele.
- [O `claude update` instala de outro jeito, fora de `~/.local/share/claude`] → O `run` seguinte recusa com a instrução de instalação, como hoje.
