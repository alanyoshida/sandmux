## Why

O change 93 trouxe as identidades de cloud, começando pelo GCP: o agente recebe um falso, e o gateway põe o real fora do sandbox. Para usar o `kubectl`, o agente ainda precisa de um kubeconfig com a credencial real (o token, o certificado de cliente, ou o plugin `exec` que a obtém), montado de propósito. Pela ordem decidida em 2026-11-22 (GCP, Kubernetes, AWS), este change é o do Kubernetes.

## What Changes

- **Identidades de Kubernetes no cofre:** `sandmux vault add-cloud kubernetes` guarda **todos os contextos** do kubeconfig do host de uma vez, cada um como uma identidade `kube:<nome>` (decisão do usuário, 2026-11-22). `--context <ctx>` (repetível) limita a alguns contextos.
  - **Referência ao host (o padrão):** o cofre guarda só o contexto e o caminho do kubeconfig do host. O daemon lê o contexto pelo `kubectl` do host a cada renovação e roda os plugins `exec` (EKS, GKE, OIDC) fora do sandbox, com o ambiente do usuário.
  - **Cópia no cofre (`--copy`):** o cluster e o usuário do contexto (o servidor, a CA, o token ou o certificado de cliente) ficam cifrados no cofre. Um usuário com plugin `exec` é recusado na cópia: o plugin só roda no host.
- **Rota pelo nome sintético (decisão do usuário):** na aba, o `server` de cada contexto vira `https://<nome>.kube.sandmux.internal`. O gateway resolve o nome, decifra o TLS com a CA do lançamento e conecta ele mesmo ao API server guardado, com a CA do cluster e, quando há, o certificado de cliente. O agente nunca escolhe para onde o real vai. Clusters no `127.0.0.1` do host (kind, minikube) funcionam.
- **Na política de rede (decisão do usuário):** o nome sintético de uma identidade passa sem regra, mesmo no modo inferência: guardar a identidade é o consentimento. Cada pedido vai ao registro com o nome da identidade e o servidor real.
- **Em toda aba, como as chaves:** com o cofre aberto, o HOME da aba tem um kubeconfig só com falsos: um contexto por identidade (com o nome e o namespace do host), um token falso por usuário e a CA do lançamento. O contexto atual é o do host, quando está entre as identidades. `kubectl config use-context` funciona, e a pasta some no fim da execução.
- **Troca no gateway:** o gateway troca o token falso pelo real (token estático, de `exec`, ou nenhum, com o certificado de cliente) e reescreve o `Host`. `kubectl exec`, `attach`, `cp` e `port-forward` funcionam: além do WebSocket, o gateway passa os outros upgrades (SPDY) como túnel.
- **Sem o real:** quando o host não dá a credencial (o `kubectl` falta, o plugin `exec` falha ou pede login), o gateway responde `401` com um `Status` do Kubernetes que diz o que fazer no host, e o daemon avisa uma vez, como no GCP.
- **Limite:** o que o agente pode fazer é o que o RBAC da identidade permitir. O gateway não filtra os pedidos, só registra.
- O protocolo entre o cliente e o daemon passa a 47 (a injeção leva o destino do Kubernetes).

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `cloud-credentials`: as identidades de Kubernetes (todos os contextos de uma vez, referência ou cópia), o kubeconfig da aba, a rota pelo nome sintético, a troca no gateway, o real fora do sandbox (`kubectl` e plugins `exec` do host) e a falta do real; a TUI mostra as identidades no domínio sintético delas.
- `tls-interception`: o destino de uma identidade de Kubernetes é verificado pela CA do cluster (não pelas CAs do sistema), com o certificado de cliente quando há; os upgrades que não são WebSocket passam como túnel.

## Impact

- `sandmux-proto`: os campos do Kubernetes em `CloudInfo` e o destino do Kubernetes em `Injection`; protocolo 47.
- O daemon (`sandmux`): `vault add-cloud kubernetes`, a leitura do kubeconfig pelo `kubectl` do host, os plugins `exec`, o cache, o kubeconfig da aba.
- `sandmux-policy`: a pasta `~/.kube` da aba e a variável `KUBECONFIG`.
- `sandmux-net`: o DNS e a política dos nomes `*.kube.sandmux.internal`, a conexão ao API server com a CA do cluster e o certificado de cliente, a resposta `401` sem o real, os upgrades como túnel.
- A TUI (seção Cofre e grafo), `vault list` e o `doctor` (o `kubectl` do host para as referências).
- Dependências: nenhum crate novo. O kubeconfig do host é lido pelo `kubectl config view --raw --flatten --minify -o json`, sem um leitor de YAML.
- Docs `docs/cloud.md` e `docs/en/cloud.md` (seção do Kubernetes), `docs/vault.md`, o README e `docs/sessions.md` (protocolo), em pt e en.
- Testes: um `kubectl` e um plugin `exec` falsos no host, e um API server local com TLS e certificado de cliente.
