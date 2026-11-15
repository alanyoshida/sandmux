## Context

São cerca de mil textos ao usuário, em oito crates. Quase todos são `format!`, `bail!`, `anyhow!`, `println!` e `eprintln!` com o texto em português.

A TUI desenha a cada quadro. O daemon atende muitos clientes. Os binários são quatro: `sandmux`, `sandmuxd`, `sandmux-run` e `sandmux-chat`.

## Goals / Non-Goals

**Goals:**
- Os dois idiomas para todo texto ao usuário.
- Troca na hora na TUI.
- Nada muda no formato do arquivo nem na linha de comando.

**Non-Goals:**
- Outros idiomas além de português e inglês. O mecanismo aceita mais, mas cada um é um change.
- Traduzir o que vem dos harnesses ou do sistema (as mensagens de `errno`).
- Traduzir nomes de valores do arquivo (`aberta`, `blocos`, `padrao`) ou os subcomandos.

## Decisions

### Texto lado a lado no código

`tr!("Proteção", "Protection")` devolve um dos dois textos; `trf!("aba {id}", "tab {id}")` faz o `format!` de um dos dois, com os mesmos argumentos.

As duas versões ficam juntas onde a mensagem é usada. Assim fica difícil esquecer uma, e a revisão vê as duas. Um catálogo com chaves (Fluent, gettext) foi descartado porque mil chaves inventadas e um arquivo à parte custam mais que a vantagem para dois idiomas.

### Idioma num estado global

`sandmux_spec::lang` guarda o idioma num `AtomicU8`; o padrão, antes de qualquer escolha, é português. Os testes de unidade, que não definem nada, continuam em português.

Cada binário chama `lang::init` ao começar, com o valor de `language` lido do `config.toml`. Se o arquivo não abre ou é inválido, vale o sistema, e o erro sai no idioma do sistema.

- **Daemon:** relê o `language` do arquivo a cada pedido, então a troca na TUI vale para as respostas dele em seguida.
- **Idioma do cliente:** a saudação (`Hello`) leva o idioma do cliente (campo `lang`, protocolo 24), e o daemon responde nesse idioma. Assim um cliente com `SANDMUX_LANG=en` recebe os erros do daemon em inglês, mesmo com o daemon iniciado em português. Ao trocar o idioma na página, a TUI reabre a conexão de controle.
- **Textos gravados no idioma do lançamento:** o resumo da proteção (nível, camada e rede) vai para o idioma atual na exibição (`launch::localize_summary`).
- **Valores do arquivo:** em inglês, a TUI e o `config show` mostram o nome traduzido de alguns valores (`aberta` como `open`), e o arquivo continua com o valor em português (`config::value_label`).
- **Supervisor (`sandmux-run`):** recebe o idioma do daemon pela variável `SANDMUX_LANG` ao ser lançado.
- **Aba de chat:** também recebe pela variável.

### Chave em `[global]`

`language` vai em `GLOBAL_ONLY_KEYS`. Ela não é política de sandbox, mas vale para o sandmux inteiro, não só para a TUI.

### Página de configurações

- **Seção nova:** "Geral" fica antes de "Aparência".
- **Item:** "Idioma" é uma escolha `auto`/`pt`/`en`; `auto` apaga a chave.
- **Textos:** os rótulos, as ajudas e as seções da página ganham a versão em inglês nos próprios itens.
- **Troca na hora:** ao gravar, a TUI chama `lang::set`, e o próximo quadro já sai no idioma novo.

### Testes

- **Ambiente:** o `Env` dos testes de integração fixa `LANG=pt_BR.UTF-8`, porque limpa o ambiente; sem isso o sistema seria `C`, isto é, inglês.
- **Testes novos:**
  - a TUI em inglês por `SANDMUX_LANG=en`;
  - a troca pela página de configurações;
  - um erro da linha de comando em inglês;
  - a precedência entre a variável, o arquivo e o sistema.

## Risks / Trade-offs

- **[Mensagem esquecida em português]** Uma busca por textos com acento e palavras comuns em português, fora de `tr!`, entra como teste de unidade em cada crate.
- **[Volume da mudança]** Mexe em quase todo arquivo. O trabalho é dividido por crate e os testes de antes, em português, garantem que nada mudou no idioma padrão.
