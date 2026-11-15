## Context

O `Ask` (change 22) roda `sandmux-chat --once` num sandbox lançado pelo daemon, só com rede de inferência. As chaves entram como falsas e o gateway troca pelo real. A lista de modelos segue o mesmo caminho, para a chave real nunca sair do daemon e do gateway.

## Decisions

- **`list_models`:** `chat::list_models(target)` faz `GET {path}/models` com `Authorization: Bearer` e lê `data[].id`. É o formato da OpenAI, que a OpenRouter, a OpenAI, a Mistral, a Groq, a xAI, a DeepSeek e os servidores locais seguem.
- **`sandmux-chat --models`:** consulta os provedores do `SANDMUX_CHAT_PROVIDERS` em paralelo, com 10 s cada. Imprime uma linha `provedor/modelo` por modelo e os avisos em stderr, com o prefixo de sempre. Termina com erro só se nenhum provedor respondeu.
- **Daemon:** `ListModels` reaproveita o lançamento do `Ask`, com os argumentos do modo. A resposta `Models { ids, notes }` leva os avisos. O tempo máximo é o mesmo do `Ask`.
- **Seletor da TUI (`tui/picker.rs`):**
  - fica por cima de qualquer modal e recebe as teclas antes dele;
  - pede a lista numa thread com uma conexão própria ao daemon, e mostra "carregando…" enquanto isso;
  - filtra por todas as palavras digitadas, sem caixa, e ordena pelo nome;
  - Enter devolve o escolhido ao destino (o campo do modal, ou a gravação do item da página);
  - a lista fica em `App` até a TUI fechar, e `Ctrl+r` (`\x12`) relê.
- **Página de configurações:** em "Modelo do chat" e "Modelo de comandos", Enter abre o seletor, e `e` edita o texto como antes. Em "Modelos sugeridos", `a` abre o seletor para acrescentar.
- **Aba de chat:** `/modelo` sem argumento pede a lista na própria aba (já está no sandbox) e abre um seletor igual.

## Risks / Trade-offs

- **[Listas grandes]** A OpenRouter tem centenas de modelos. O filtro por palavras resolve, e a lista é rolada.
- **[Lançar um sandbox para listar]** Leva perto de um segundo, por isso a lista fica guardada.
