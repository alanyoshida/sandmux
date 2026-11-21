## Why

Numa aba confinada pelo kernel (change 78), o comando não tem o filtro de aberturas: o BPF LSM decide e registra. O shell de entrada (`sandmux enter`, Ctrl+b `t`) dessa aba ainda instala o filtro e passa cada abertura pelo supervisor. Assim, ele não vê a aba do mesmo jeito que o agente:

- uma leitura em massa no shell custa de 3x a 3,5x, contra 1,05x a 1,14x no agente;
- o supervisor emula as aberturas (o `/proc/self`, os namespaces aninhados, o `lsof`), e o agente não passa por essa emulação;
- o shell tem um filtro seccomp a mais que o agente.

O shell existe para mostrar exatamente o que o agente vê (change 41). A divergência está escrita na documentação desde o change 78.

## What Changes

- O shell de entrada segue o modo de arquivos da aba alvo. Numa aba pelo kernel, ele não instala o filtro de aberturas, e as aberturas dele são decididas e registradas pelo kernel, como as do agente. Numa aba pelo supervisor, nada muda.
- O filtro das execuções segue a mesma regra do comando: ele continua no shell quando o registro das execuções da aba é pelo supervisor.
- O shell já roda no escopo do systemd da aba, onde estão presos os programas BPF. Por isso não muda nada no daemon nem no auxiliar: só o lançador, que passa o modo da aba ao auxiliar de entrada.
- Um pedido de entrada que chega antes de a aba ter o modo espera a resposta do daemon. Sem resposta, o shell fica com o supervisor, que é o recuo de hoje.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `terminal-sessions`: o requisito "Shell dentro do sandbox de uma aba" passa a dizer que as aberturas do shell seguem o caminho da aba alvo (o kernel ou o supervisor), com um cenário para a aba pelo kernel.

## Impact

- `crates/sandmux-launcher/src/enter.rs`: o pedido de shell leva o modo de arquivos da aba, e o shell instala os filtros como o comando.
- `crates/sandmux-launcher/src/launch.rs`: a thread dos arquivos guarda o modo da aba, e a thread de entrada o passa a cada pedido.
- O protocolo entre o lançador, o daemon e o auxiliar não muda. Uma aba lançada por uma versão anterior continua com o supervisor no shell até ser reiniciada.
- Testes:
  - no host, sem BPF: o shell de entrada de uma aba pelo supervisor continua com as aberturas pelo supervisor;
  - com root, na VM, no 6.8 e no 7.0: numa aba pelo kernel, o shell de entrada nega o `.env` e o `file-deny`, as leituras dele entram no registro, e ele tem o mesmo número de filtros seccomp que o agente.
- Documentação: `docs/files.md` ("Pelo kernel") e `docs/sessions.md` (o shell de entrada), com os pares em inglês.
