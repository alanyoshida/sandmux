sandmux 0.1.38, antes da v1: o agente usa o gcloud sem que uma credencial real do Google Cloud entre no sandbox.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidade

- **Credenciais de cloud, começando pelo GCP:** `sandmux vault add-cloud gcp <nome>` guarda uma identidade no cofre. A fonte pode ser uma configuração do `gcloud` do host (`--configuration`) ou uma cópia cifrada do ADC (`--copy-adc`) ou da chave de uma conta de serviço (`--copy-key`).
  - Cada aba recebe uma configuração do `gcloud` e um ADC só com tokens falsos.
  - O gateway troca o falso pelo real nas APIs do Google e responde sozinho o pedido de token. O real fica só na memória do daemon e é renovado antes de vencer.
  - O que o agente pode fazer é o que o IAM da identidade permitir. Use uma identidade sem permissão de criar chaves de conta de serviço nem de ler o Secret Manager.
  - As bibliotecas por gRPC ainda não funcionam. Detalhes em [docs/cloud.md](https://github.com/alanyoshida/sandmux/blob/main/docs/cloud.md).

O protocolo entre o cliente e o daemon passa a 46: depois de atualizar, rode `sandmux daemon restart`. O auxiliar root não muda.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
