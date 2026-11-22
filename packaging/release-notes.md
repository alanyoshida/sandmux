sandmux 0.1.39, antes da v1: o agente usa o kubectl sem que uma credencial real do Kubernetes entre no sandbox.

The interface is also available in English: see [README.en.md](https://github.com/alanyoshida/sandmux/blob/main/README.en.md).

## Novidade

- **Credenciais do Kubernetes:** `sandmux vault add-cloud kubernetes` guarda todos os contextos do kubeconfig do host (ou só os de `--context`), cada um como uma identidade `kube:<nome>`.
  - Por referência (o padrão), o daemon lê o contexto pelo `kubectl` do host e roda fora do sandbox os plugins `exec` (EKS, GKE, kubelogin). Com `--copy`, o servidor, a CA e o token ou o certificado de cliente ficam cifrados no cofre.
  - Cada aba recebe um kubeconfig só com falsos, com o servidor `https://<nome>.kube.sandmux.internal`. O gateway conecta ele mesmo ao API server guardado, inclusive o `127.0.0.1` do host (kind, minikube), com a CA do cluster e o certificado de cliente.
  - `kubectl get` e `kubectl exec` foram verificados contra um cluster kind; `logs -f` e `port-forward` passam pelo mesmo caminho. Sem o real, o `kubectl` recebe `401` com o motivo.
  - O que o agente pode fazer é o que o RBAC do contexto permitir. Tire do cofre os contextos de produção que o agente não deve ver. Detalhes em [docs/cloud.md](https://github.com/alanyoshida/sandmux/blob/main/docs/cloud.md).

O protocolo entre o cliente e o daemon passa a 47: depois de atualizar, rode `sandmux daemon restart`. O auxiliar root não muda.

## Instalar

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

O script pede `sudo` para instalar em `/usr/local/bin` e o perfil do AppArmor do lançador; sem `sudo`, instala em `~/.local/bin`. Também há `.deb`, `.rpm` e o tarball. Depois de instalar por pacote ou por `cargo install`, rode `sandmux setup`.

Requisitos: Linux `x86_64` com kernel 6.12 ou mais novo (Landlock ABI 6), ou o 6.8 do Ubuntu 24.04 com o auxiliar root, e user namespaces sem privilégio. Testado no Ubuntu 26.04, no Ubuntu 24.04 (numa VM) e em contêineres do Debian 13, do Fedora atual e do Arch. Detalhes em [docs/install.md](https://github.com/alanyoshida/sandmux/blob/main/docs/install.md).
