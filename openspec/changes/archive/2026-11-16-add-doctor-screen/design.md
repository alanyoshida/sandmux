## Context

- `crates/sandmux/src/doctor.rs` imprime direto no terminal. As verificações de suporte vêm de `support::check_via` (Landlock, user namespace, overlay), e o resto é lido na hora: tetos, Docker, cache de imagens, TUN, diretório de dados, `config.toml`, daemon, cofre e chaves reais nos HOMEs sintéticos. O `sandmux doctor` instalado leva cerca de 70 ms nesta máquina.
- `doctor --fix` (`fix_secrets`) lê as chaves reais (`real_secrets`) e o login do `agy` (`agy_login`). Ele abre o cofre se preciso e, arquivo a arquivo, guarda cada chave com `VaultAdd` (falso e domínios de `infer_domains_for`, ou perguntado). Só depois de o cofre aceitar todas as chaves do arquivo é que grava os falsos (`apply_fakes`). O login do `agy` vai por `vaultcmd::store_agy_login`.
- A página de configurações (`tui/settings.rs` e `render.rs`) tem seções de itens do `config.toml` e uma seção especial, o Cofre (`VAULT = 9`). O Cofre tem estado próprio (`VaultView`), teclas próprias (`vault_key`) e formulários com campos apagados da memória (`VaultForm`).
- Os testes de integração conferem trechos da saída do `sandmux doctor` em português (`[ok  ] …`, `[FALHA] …`, `estado: parado`, `Chaves reais dentro do sandbox (3)` e outros).
- A TUI e o `doctor` estão no mesmo binário (`crates/sandmux`), então a TUI chama o `doctor` direto.

## Goals / Non-Goals

**Goals:**
- Ver o diagnóstico e mover as chaves reais sem sair da TUI.
- Uma única fonte das verificações para o terminal e a TUI, sem mudar a saída do terminal.

**Non-Goals:**
- Copiar comandos de correção para o clipboard: não foi pedido.
- Rodar correções que precisam de `sudo`, como o perfil do AppArmor.
- Usar a sugestão pela URL próxima no vigia do daemon, que move chaves sem confirmação.

## Decisions

### Relatório em dados
`doctor::report() -> Report` monta `Report { groups: Vec<Group>, can_launch }`, com `Group { name, checks: Vec<Check> }` e `Check { state, label, summary, help, cli }`:

- `state` é `Ok`, `Fail` ou `Info`;
- `label` e `summary` formam a linha da TUI;
- `help` traz a causa e a correção, mostradas embaixo;
- `cli` é o texto exato que o terminal imprime para aquela verificação.

`run_with(false)` imprime os `cli` em ordem e devolve o código pelo `can_launch`. Guardar o texto do terminal em cada verificação mantém a saída idêntica, e os testes atuais servem de regressão. Uma alternativa era formatar o terminal a partir dos campos, mas isso mudaria quebras de linha e recuos que os testes conferem.

O `help` do AppArmor não traz o perfil inteiro, que tem dezenas de linhas. Ele diz que `sandmux doctor` no terminal mostra o comando para colar.

### Seção Diagnóstico
É a seção 10, depois do Cofre (`DOCTOR = 10`), com `DoctorView { report, sel, fix, form }` em `Settings`. O relatório é lido:

- ao abrir a página, para a contagem de falhas aparecer no nome da seção na lateral ("Diagnóstico ✗1");
- ao entrar na seção;
- com `r`.

Na lista, cada linha mostra `✓`, `✗` ou `·`, o rótulo e o resumo, sob o nome do grupo. A área de baixo mostra o `help` da linha escolhida, com quebra de linha.

### Ação `doctor`
`("doctor", 'D')` entra em `TUI_ACTIONS`, na ajuda e no menu de ações. Ela abre a página com `section = DOCTOR` e o foco na lista. `D` não tem outro uso.

### Mover as chaves (`f`)
1. A TUI relê `real_secrets` e `agy_login`. Sem nenhum dos dois, mostra "nada a mover".
2. Sem cofre, avisa para criar o cofre na seção Cofre. Com o cofre trancado, abre o formulário de senha (`FormKind::Unlock`) e, depois de desbloquear, continua.
3. Para cada arquivo, uma chave por vez: o formulário "Domínios de `<nome>`" vem com o campo preenchido, e o usuário pode editá-lo. O campo vem de `infer_domains_for` ou, sem resultado, da sugestão pela URL próxima; sem nenhuma das duas, vem vazio.
   - Enter guarda a resposta e passa à próxima chave.
   - Esc pula o arquivo inteiro, e nada dele vai ao cofre.
   - Depois da última chave do arquivo, a TUI manda os `VaultAdd` com os falsos. Só se todos forem aceitos grava os falsos com `apply_fakes`; uma recusa deixa o arquivo como está e mostra o motivo.
   - Um campo vazio guarda a chave sem domínio, como o `doctor --fix` (sem troca no tráfego), e o formulário avisa disso.
4. O login do `agy` pede "s confirma · outra tecla pula" e vai por `store_agy_login`.
5. No fim, um aviso com quantas chaves foram levadas e as abas a reiniciar (Ctrl+b r). O relatório é relido.

Juntar as respostas antes de mandar ao cofre evita deixar no cofre metade das chaves de um arquivo que o usuário pulou. Os valores reais ficam só na estrutura lida (como no `doctor --fix`), não aparecem na tela e são apagados com o fluxo.

### Sugestão pela URL próxima
`secretdomains::nearby_url_domain(text, line)` procura, nas até 10 linhas antes e depois da chave, a URL `http(s)://` mais próxima e devolve o host normalizado (`localhost` vira `host.sandmux.internal`). Ela só preenche o campo na TUI. `infer_domains_for` não muda, porque o vigia do daemon move chaves sem confirmação e uma URL próxima pode ser de outro serviço.

## Risks / Trade-offs

- [O `cli` de cada verificação duplica o texto do resumo] → Os dois são montados no mesmo lugar, e os testes do terminal continuam conferindo o texto.
- [A sugestão pela URL próxima pode apontar o serviço errado] → O campo é sempre confirmado pelo usuário, e o domínio só decide para onde o real vai.
- [Rodar o relatório ao abrir a página atrasa a abertura] → Cerca de 70 ms; o `check_via` é a parte mais lenta, e o resto são leituras de arquivo e uma conexão ao daemon.
- [Os valores reais ficam na memória da TUI durante o fluxo] → É o mesmo que o `doctor --fix` faz; o fluxo descarta a estrutura ao terminar ou ao sair.
