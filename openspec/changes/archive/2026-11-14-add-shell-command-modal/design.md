## Context

O change 21 deixou pronto:

- `Msg::Ask`, que roda o `sandmux-chat --once` numa pasta temporária, confinado só com inferência, com a chave do cofre trocada no gateway;
- a tabela de provedores, `[assistant]` e a conferência do cofre e da chave (`ChatCheck`) que o modal de chat usa.

As decisões 2.7 do roadmap valem aqui:

- o comando gerado só é escrito na linha do terminal;
- vão ao modelo só a pergunta, a pasta atual e o shell;
- há um modelo padrão para comandos, trocável no modal.

## Goals / Non-Goals

**Goals:**
- Modal de comando sobre a aba ativa, com a resposta editável e a escrita na aba sem executar.
- A TUI não trava enquanto o modelo responde.
- Nenhuma linha da tela da aba vai ao modelo.

**Non-Goals:**
- Executar o comando. O usuário sempre aperta Enter na aba.
- Incluir as últimas linhas do terminal na pergunta. Está em "Depois da v1" no roadmap ("Tela da aba como contexto do assistente").
- Descobrir a pasta atual do shell dentro do sandbox. Vai a pasta de trabalho da aba (ver Riscos).
- Explicação do comando. O texto de sistema pede só o comando.

## Decisions

### A pergunta passa pelo `ask` do change 21, com texto de sistema

`Msg::Ask` ganha `system: Option<String>` (`serde default`). O daemon grava o texto na conversa da pasta temporária (`Conversa.sistema`), e o `sandmux-chat` o manda como a primeira mensagem, com o papel `system`. Assim a pergunta herda o confinamento, a troca da chave e o registro de rede, sem caminho novo.

O texto de sistema é montado pelo cliente, em `sandmux_policy::assistant::command_prompt(pasta, shell)`. Ele traz a pasta e o shell e pede:

- um único comando para aquele shell;
- sem explicação e sem cercas de código;
- várias linhas só quando não houver outro jeito.

A pergunta vai como a mensagem do usuário, sem mudança.

**Alternativa considerada:** pedir ao daemon um "comando" com campos próprios (pasta, shell). Mais acoplamento entre o protocolo e um uso só. O texto de sistema genérico também serve a usos futuros.

### O shell e a pasta

- **Pasta:** a pasta de trabalho da sessão da aba ativa (`Tab.workdir`).
- **Shell:** o nome do executável do shell que o sandbox usa para a aba. A TUI não sabe qual shell roda dentro de um harness, então usa o `SHELL` do cliente quando ele está nas pastas de sistema, e `bash` quando não está. É a mesma regra de `presets::system_shell`.

### A pergunta numa thread

O modal guarda um estado (`Pergunta`, `Esperando`, `Comando`). O Enter abre uma conexão própria com o daemon numa thread e manda o `ask`. A resposta volta ao laço da TUI como um evento `Ev::Answer(token, Result)`. O `token` descarta respostas de um modal já fechado ou refeito. Esc durante a espera fecha o modal, e a resposta que chegar depois é descartada.

### Conferência antes de perguntar

É a mesma do modal de chat (`chatcmd::check`): provedor conhecido, cofre e chave. Com o cofre trancado, o desbloqueio abre antes e a pergunta segue depois, como na aba de chat (`after_unlock` passa a guardar o tipo do pedido).

### Limpeza da resposta

`assistant::clean_command(texto)`:
1. tira os espaços das pontas;
2. se houver um bloco de código, pega o conteúdo do primeiro;
3. tira um `$ ` ou `% ` no começo de cada linha;
4. tira as linhas vazias das pontas.

### Escrita na aba

Com o comando de uma linha, os bytes vão direto pela conexão da aba (`send_active`), sem `\r`.

Com várias linhas:
- se a tela da aba ligou o modo de colagem (`screen.bracketed_paste()`), o comando vai entre `ESC[200~` e `ESC[201~`;
- sem ele, o modal recusa.

Isso vale para qualquer aba. Num harness que não é shell, o texto também é só escrito na entrada dele.

### Ação e atalho

A ação se chama `command`, na tecla `g` ("gerar"), porque o `c` já é a nova aba.

### Protocolo 21

`Ask.system`. Um daemon do protocolo 20 ignoraria o campo e responderia sem o texto de sistema. A troca de número evita essa diferença silenciosa.

## Risks / Trade-offs

- **[A pasta enviada é a de trabalho, não a atual do shell]** Num shell que mudou de pasta, o modelo recebe a pasta em que a aba começou. Para a maioria dos comandos isso não muda a resposta. Descobrir a pasta atual exigiria ler o processo em primeiro plano do PTY no daemon; fica como melhoria possível.
- **[O modelo pode devolver texto além do comando]** A limpeza tira cercas e prompts. O campo é editável e nada roda sem o Enter do usuário.
- **[Comando perigoso sugerido]** O comando roda dentro do sandbox da aba, e o usuário o lê antes de executar.
