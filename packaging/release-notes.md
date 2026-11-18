sandmux 0.1.21, antes da v1: o Docker confinado acompanha a pasta ao vivo.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

Com o confinamento do Docker privado por eBPF ligado (o auxiliar `sandmux-helper`, da 0.1.20):

- **Arquivos novos negados na hora:** um `.env` ou outro segredo criado com o Docker rodando, e qualquer arquivo que aparece numa pasta de `file-deny`, ficam negados aos contêineres em até 1 segundo, sem subir o Docker de novo. Um segredo que o próprio agente cria continua legível.
- **Política de arquivos ao vivo:** uma mudança em `file-deny`, `file-allow`, `secret-files` ou `secret-files-off` vale no Docker que já está rodando.
- **Registro dos contêineres:** as tentativas negadas e os arquivos criados pelos contêineres aparecem no registro de arquivos e em `sandmux files <id>`, com o caminho do host.
- Detalhes em [docs/docker.md](https://github.com/alanyoshida/sandmux/blob/main/docs/docker.md#ao-vivo).

Esta versão muda o protocolo entre o cliente e o daemon (40) e o auxiliar root. Depois de atualizar, rode `sandmux setup` (que reinicia o auxiliar) e `sandmux daemon restart`, e abra a TUI de novo.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
