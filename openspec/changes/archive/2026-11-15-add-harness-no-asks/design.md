## Context

A composição (`sandmux-policy/src/compose.rs`) monta o comando do harness pelo `presets::plan`, com os argumentos dados depois de `--`, e o ambiente do zero. O opencode já recebe configuração por variável (`OPENCODE_CONFIG_CONTENT`, para os MCPs). Os mecanismos foram conferidos nos binários e no código instalados nesta máquina, sem rodar os harnesses:

- opencode 1.19.5: lê `OPENCODE_PERMISSION` (JSON) e mescla por cima de `permission` da configuração. O padrão dele pergunta em `external_directory`, `doom_loop` e na leitura de `*.env`.
- Claude Code 2.1.325: `--dangerously-skip-permissions`; `--settings <arquivo-ou-json>`; a configuração `skipDangerousModePermissionPrompt`.
- hermes: `HERMES_YOLO_MODE` pula as aprovações de comandos perigosos; as regras fixas de bloqueio continuam.
- oh-my-pi 18.12.6: `--auto-approve` e `--yolo` levam ao modo `yolo` de aprovação.

## Goals / Non-Goals

**Goals:**
- O agente trabalha sem interrupções de permissão; o sandbox decide o que é possível.
- Uma chave devolve as perguntas.

**Non-Goals:**
- Opções do `run` para a chave: ela é de política, como as outras de configuração; a precedência entre global, perfil e projeto basta.
- Mexer nos arquivos de configuração do harness no HOME sintético: o ajuste vai pelo ambiente e pelos argumentos de cada lançamento, e some com a chave.

## Decisions

- **Chave de liga e desliga `harness-asks`,** padrão `false`, com a precedência das outras chaves de liga e desliga.
- **opencode:** `OPENCODE_PERMISSION={"*":"allow","edit":"allow","bash":"allow","webfetch":"allow","read":"allow","external_directory":"allow","doom_loop":"allow"}`. Como o opencode mescla a variável por cima da configuração, ela vence o `opencode.json` do usuário; a chave do sandmux é o jeito de voltar. As permissões próprias dos agentes (o `plan`, que não edita) continuam.
- **Claude Code:** `--dangerously-skip-permissions --settings {"skipDangerousModePermissionPrompt":true}` logo depois do executável, antes dos argumentos do usuário.
- **hermes:** `HERMES_YOLO_MODE=1` no ambiente.
- **oh-my-pi:** `--auto-approve` logo depois do executável.
- **Onde:** na composição, depois do plano do preset, pelo nome do harness. O `shell` e o `chat` não mudam.

## Risks / Trade-offs

- [Um subcomando do harness depois das opções (`claude mcp list` com `--dangerously-skip-permissions` antes)] → As opções são globais nos dois harnesses (commander e o parser do omp aceitam opções antes do subcomando). Se algum subcomando recusar, `harness-asks = true` contorna.
- [O Claude Code ainda pedir a confirmação do modo uma vez] → Se a versão instalada não ler `skipDangerousModePermissionPrompt` do `--settings`, a confirmação aparece uma vez e fica gravada no HOME sintético.
- [Versões futuras mudarem o mecanismo] → Os nomes ficam num só lugar da composição, e os testes conferem o que é passado.
