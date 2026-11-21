sandmux 0.1.32, antes da v1: avisa quando uma chave move para o cofre e mostra a árvore de processos da aba.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Aviso de chave movida ao cofre:** quando o vigia de segredos move uma chave real para o cofre, a TUI mostra um modal por cima de qualquer aba, mesmo que não seja a afetada, listando cada sessão que precisa reiniciar. Reinicia uma sessão ou todas as listadas, sem trocar de aba sozinha.
- **Árvore de processos na página de configurações:** a seção "Processos" (depois de Diagnóstico) mostra, para a aba ativa, a árvore de processos do último lançamento, com zoom de densidade (nome e pid; mais memória; ou um cartão com cmdline, CPU e cgroup), dobra manual e automática de ramos grandes, e drill-down com migalha de pão. Precisa do auxiliar root; sem ele, a seção mostra o motivo.

Esta versão muda o cliente, o daemon e o auxiliar root. O protocolo entre o cliente e o daemon sobe para 45: depois de atualizar, rode `sandmux daemon restart`, e atualize o auxiliar (`sandmux setup`).

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
