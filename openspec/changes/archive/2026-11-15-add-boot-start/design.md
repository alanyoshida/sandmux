## Context

- **A unidade** `sandmuxd.service` é instalada em `~/.config/systemd/user` com `WantedBy=default.target`. Com o linger, o gerenciador do usuário sobe no boot e a inicia.
- **O logind** mostra o estado do usuário em `/run/systemd/users/<uid>`: `STATE=lingering` sem sessões, `online` ou `active` com login. O linger aparece como o arquivo `/var/lib/systemd/linger/<usuário>`.
- **A restauração** (`restore`) recalcula o estado de cada sessão guardada e relança as vivas.

## Goals / Non-Goals

**Goals:**
- Um comando para o início no boot.
- Nenhuma aba antes do login (decisão do usuário), e todas de volta no primeiro login, sem ação do usuário.
- Nenhuma consulta ao keyring sem login.

**Non-Goals:**
- Abrir o cofre antes do login. O keyring só abre com a sessão do usuário.
- Escolher, por aba, quais voltam no boot.
- Sistemas sem systemd e logind. Sem o arquivo do logind, o daemon trata o usuário como logado e restaura como hoje.

## Decisions

- **Estado do login pelo arquivo do logind,** não pelo `loginctl` nem pelo D-Bus do sistema: uma leitura de arquivo, sem processo nem dependência. Contam como login `active` e `online`.
  - `SANDMUX_LOGIND_USER` troca o caminho, para os testes.
  - Sem o arquivo, o usuário conta como logado.
- **`AguardandoLogin`,** uma variante nova de `SessionState`. Ela é guardada na lista, e assim um reinício antes do login não perde as abas.
  - Na restauração, uma sessão gravada nesse estado vale como viva: com login, é relançada; sem login, continua esperando.
  - As que iriam para "aguardando o cofre" também esperam o login. Depois do login, voltam a esperar o cofre como hoje, porque a restauração recalcula o estado.
- **A thread `espera-login`** existe só quando o daemon sobe sem login:
  - lê o arquivo a cada 2 segundos;
  - no login, recalcula as sessões que esperavam, com a mesma lógica da restauração, e relança as que devem voltar;
  - depois disso, tenta o keyring, avisa o cofre trancado 5 segundos depois e termina.
- **Keyring e aviso do cofre na subida** só quando o usuário está logado.
- **`daemon boot`:**
  - roda `loginctl enable-linger` ou `disable-linger` (o `loginctl` resolve o usuário atual), com `SANDMUX_LOGINCTL` para os testes;
  - no `on`, também instala e habilita a unidade como o `setup` faz;
  - o estado vem do arquivo do linger (`SANDMUX_LINGER_DIR` para os testes) e do `systemctl --user is-enabled`.
- **Mensagens:** "aguardando o login" no `ls` e `(login)` como marca da aba na TUI.
- **Protocolo 31.**

## Risks / Trade-offs

- [O `loginctl enable-linger` pede autorização do polkit em algumas distros] → O comando mostra o erro do `loginctl`. O usuário pode rodar `sudo loginctl enable-linger $USER` por conta própria, e o `daemon boot` passa a mostrar ligado.
- [Login só por SSH] → Também é login (`online`), e as abas voltam. O keyring da sessão SSH em geral fica trancado, e o cofre continua pedindo a senha.
- [O `off` desliga o linger de todos os serviços do usuário] → O comando avisa antes, na própria saída. Ninguém o roda sem querer.
