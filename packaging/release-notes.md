sandmux 0.1.17, antes da v1: o login do Google do `agy` fora do sandbox e o diagnóstico na TUI.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Login do `agy` fora do sandbox:** o token real do login do Google não fica mais no HOME do sandbox. O agente só vê valores falsos, e o gateway põe o real só no tráfego ao Google, nas abas do `agy`.
  - Com o cofre aberto, `sandmux import agy` (ou a primeira execução) leva o login do host ao cofre como `agy:access` e `agy:refresh`, e o `agy` abre já logado. Com o cofre trancado, rode `sandmux import agy --overwrite` depois de abri-lo.
  - Sem login no host, o login feito dentro do `agy` (colando o código da página) é capturado pelo gateway e vai ao cofre.
  - Um login real que tenha ficado no sandbox vai sozinho ao cofre quando ele abre, ou pelo `sandmux doctor --fix`.
  - O modo de rede só inferência passa a liberar os hosts do login e do modelo do `agy`. Detalhes em [docs/agy.md](https://github.com/alanyoshida/sandmux/blob/main/docs/agy.md#login-do-google).
- **Diagnóstico na TUI:** a página de configurações ganhou a seção Diagnóstico, aberta também por Ctrl+b `D`, com as verificações do `sandmux doctor` e a causa e a correção de cada falha. A tecla `f` leva as chaves reais do sandbox ao cofre, com o domínio de cada uma num campo que dá para editar.
- **Rodapé das configurações:** mostra só as teclas que valem no momento, conforme a seção e o item escolhido.

Esta versão muda o protocolo entre o gateway e o daemon (36). Depois de atualizar, rode `sandmux daemon restart` e abra a TUI de novo.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
