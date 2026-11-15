sandmux 0.1.4, antes da v1: chaves reais levadas ao cofre sozinhas e atualização dos harnesses pelo sandmux.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidades

- **Chaves reais movidas sozinhas:** quando você conecta um provedor pelo próprio harness (o "Connect provider" do opencode, o assistente do hermes), a chave real ia para um arquivo do HOME sintético, ao alcance do agente, até alguém rodar `sandmux doctor --fix`. Agora o daemon faz isso sozinho:
  - observa os arquivos de credencial e, segundos depois da gravação, leva a chave ao cofre e deixa um falso no lugar;
  - também varre ao subir, a cada 60 segundos e logo depois de o cofre ser aberto;
  - com o cofre trancado, ou com uma chave de domínio desconhecido, o arquivo fica como está e a aba avisa. As que esperam o cofre vão quando ele abre; as sem domínio ficam para o `sandmux doctor --fix`;
  - a aba avisa para reiniciar, porque o harness ainda tem a chave real na memória. O aviso aparece no `sandmux ls`, na marca `✱` da aba e no painel da TUI.

  Detalhes em [docs/vault.md](https://github.com/alanyoshida/sandmux/blob/main/docs/vault.md#movimento-automático).
- **`sandmux update <harness>`:** roda no host, fora do sandbox, o atualizador oficial do harness (`opencode upgrade`, `hermes update`, `claude update`, `omp update`). Com abas vivas do harness, pergunta antes (`--yes` pula) e depois diz quais reiniciar. Detalhes em [docs/sessions.md](https://github.com/alanyoshida/sandmux/blob/main/docs/sessions.md#atualizar-o-harness).

Depois de atualizar, rode `sandmux daemon restart` (o protocolo passou a 27).

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6) e user namespaces sem privilégio. Testado no Ubuntu 26.04, e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
