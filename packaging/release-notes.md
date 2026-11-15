sandmux 0.1.3, antes da v1: Claude Code e oh-my-pi confinados, e a tela da aba como contexto do modal de comando.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Claude Code:** o preset `claude` roda o Claude Code instalado no host (`~/.local/bin/claude`) dentro do sandbox, com o binário em somente leitura e a atualização automática desligada. O login da assinatura é feito dentro do sandbox, e os tokens dele ficam no cofre: o harness só vê valores falsos, e o gateway troca pelos reais no tráfego. A importação copia a configuração, sem o `.credentials.json`. Detalhes em [docs/claude.md](https://github.com/alanyoshida/sandmux/blob/main/docs/claude.md).
- **oh-my-pi:** o preset `omp` roda o oh-my-pi do host (`~/.local/bin/omp`) dentro do sandbox. As chaves dos provedores vêm do cofre, como no opencode e no hermes. A importação copia `config.yml`, extensões, agentes, regras e skills de `~/.omp/agent`, sem o `agent.db` com as credenciais. Detalhes em [docs/omp.md](https://github.com/alanyoshida/sandmux/blob/main/docs/omp.md).
- **Tela da aba no modal de comando:** no modal de comando da TUI, o campo **tela** (Tab até ele e Espaço) envia as últimas 40 linhas visíveis da aba junto com a pergunta, para pedir o comando que corrige um erro que acabou de aparecer. Começa desligado a cada abertura, e o modal avisa que a tela vai ao provedor.

Depois de atualizar, rode `sandmux daemon restart`.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
