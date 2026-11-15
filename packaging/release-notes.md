sandmux 0.1.1, antes da v1: correções do primeiro uso, página de configurações, idioma inglês e cofre na TUI.

The interface is now available in English too: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Portas do host:** `host-ports` leva portas da sua máquina ao `127.0.0.1` do sandbox, para servidores de modelo locais (LM Studio, Ollama, Unsloth) e MCPs locais. A chave do servidor fica no cofre e é trocada também em HTTP.
- **Modelos locais no opencode:** os modelos do servidor local aparecem sozinhos, lidos de `/models`.
- **Clipboard:** a cópia feita no harness chega ao clipboard do sistema.
- **Página de configurações** (Ctrl+b `e`): as opções do `config.toml` como toggles, escolhas e listas, gravadas na hora. Também:
  - um menu de ações (Ctrl+b `:`);
  - temas prontos: Catppuccin, Rosé Pine, Tokyo Night, Kanagawa, Gruvbox e Dracula.
- **Barra e painel:**
  - a barra em blocos funciona com qualquer fonte; o estilo Powerline fica opcional;
  - o painel lateral tem seções que recolhem e mudam de ordem, com medidores de uso.
- **Idioma:** português ou inglês em tudo, inclusive a documentação; segue o sistema ou a opção `language`.
- **Cofre na TUI:** criar, desbloquear, trancar, trocar a senha, acrescentar segredos e chaves SSH, editar domínios e apagar, sem nunca mostrar os valores.
- **Seletor de modelo:** a lista dos modelos dos provedores (`/models`) para escolher o modelo do chat e dos comandos.
- **`sandmux doctor --fix`:** leva ao cofre as chaves reais encontradas dentro do sandbox.

O protocolo entre o cliente e o daemon mudou (25): depois de atualizar, rode `sandmux daemon restart`.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
