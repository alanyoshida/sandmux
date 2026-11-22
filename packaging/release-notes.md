sandmux 0.1.40, antes da v1: o agente usa o aws e os SDKs da AWS sem que uma chave real entre no sandbox.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidade

- **Credenciais da AWS:** `sandmux vault add-cloud aws` guarda todos os perfis do host (ou só os de `--profile`), cada um como uma identidade `aws:<nome>`.
  - Por referência (o padrão), o daemon tira o real do `aws configure export-credentials` do host, com SSO, `assume-role` e `credential_process`. Com `--copy`, a chave de longo prazo de um usuário IAM fica cifrada no cofre.
  - Cada aba recebe uma pasta `~/.aws` só com pares falsos. O gateway refaz a assinatura SigV4 de cada pedido com a chave real, também nas URLs pré-assinadas e nos uploads do S3 em pedaços assinados, e põe o token de sessão real.
  - As credenciais que o STS (`assume-role`, `get-session-token`), o SSO e o Cognito devolvem dentro da aba viram falsos; o real fica só na memória do daemon.
  - Sem o real (o SSO venceu, por exemplo), a aba recebe `403` com o que fazer no host.
  - O que o agente pode fazer é o que o IAM do perfil permitir: use um papel sem `iam:CreateAccessKey` nem leitura do Secrets Manager. Ainda não foi testado contra a AWS de verdade. Detalhes em [docs/cloud.md](https://github.com/alanyoshida/sandmux/blob/main/docs/cloud.md).

O protocolo entre o cliente e o daemon passa a 48: depois de atualizar, rode `sandmux daemon restart`. O auxiliar root não muda.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
