sandmux 0.1.41, antes da v1: o gateway decifra HTTP/2, e as bibliotecas gRPC funcionam num domínio decifrado.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidade

- **HTTP/2 e gRPC no gateway:** nas conexões decifradas, o gateway passa a falar HTTP/2 quando o cliente e o destino falam.
  - O protocolo segue o cliente. Um cliente que só oferece HTTP/1.1 (um WebSocket, o `kubectl exec`) continua em HTTP/1.1.
  - Cada pedido HTTP/2 passa pela mesma troca de chaves do cofre, pela mesma reassinatura da AWS e pelo mesmo registro.
  - As bibliotecas do Google por gRPC (Pub/Sub, Firestore, Spanner) voltam a funcionar numa aba com uma identidade de GCP.
  - `http2 = false` na configuração volta ao HTTP/1.1. Detalhes em [docs/network.md](https://github.com/alanyoshida/sandmux/blob/main/docs/network.md).

O protocolo entre o cliente e o daemon não muda, e o auxiliar root também não.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
