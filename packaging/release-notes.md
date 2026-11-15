sandmux 0.1.2, antes da v1: shell dentro do sandbox de uma aba e proteção dos arquivos que o host executa.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Shell dentro da aba:** Ctrl+b `t` na TUI, ou `sandmux enter [id]`, abre um shell dentro do sandbox de uma aba viva. Ele vê exatamente o que o agente vê:
  - os mesmos arquivos, a mesma rede, o mesmo Landlock e o mesmo seccomp;
  - as mesmas regras de arquivos (um `.env` continua negado);
  - os mesmos processos.
- **Arquivos que o host executa:** a escrita em `~/.bashrc`, `~/.profile`, `~/.zshrc`, `~/.local/bin`, `~/.config/autostart`, `~/.config/systemd` e afins passa a ser recusada em `--rw`, na chave `rw`, na pasta de trabalho e nas pastas montadas ao vivo. Com escrita neles, o agente poderia plantar algo que roda fora do sandbox. `--ro` continua aceito.
- **Teclas:** uma tecla escolhida em `[tui.keys]` vence a tecla padrão de outra ação. Assim uma ação nova não invalida a configuração.

Depois de atualizar, rode `sandmux daemon restart`. As abas abertas por uma versão anterior precisam ser reiniciadas (Ctrl+b `r`) para aceitar o Ctrl+b `t`.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
