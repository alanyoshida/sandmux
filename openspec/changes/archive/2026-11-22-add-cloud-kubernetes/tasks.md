Os testes rodam no host, sob o vigia de memória. Nenhum teste fala com um cluster de verdade: o `kubectl` do host e o plugin `exec` são scripts falsos (`SANDMUX_KUBECTL`), e o API server é um servidor local com TLS de uma CA de teste e, num caso, com o certificado de cliente exigido.

## 1. Protocolo e cofre

- [x] 1.1 Em `sandmux-proto`, os campos `server`, `namespace` e `kubeconfig` em `CloudInfo`, e `KubeTarget` (`server`, `ca_pem`, `server_name`, `insecure`, `cert_pem`, `key_pem`, com o `Debug` sem a chave) em `Injection.kube`, todos com `serde(default)`; protocolo 47. Teste de unidade: uma injeção e um `CloudInfo` do protocolo 46 ainda leem
- [x] 1.2 Em `sandmux-spec`, `KUBE_SUFFIX` (`kube.sandmux.internal`), o nome de uma identidade a partir do contexto (minúsculas, `[a-z0-9-]`, sem `-` repetido nem nas pontas, até 40) e a validação do domínio sintético no `Pattern`. Testes de unidade com contextos do kind, do EKS (ARN) e do GKE
- [x] 1.3 No daemon, `add_cloud` aceita o provedor `kubernetes`: o falso `sandmux-fake-kube-…`, o domínio `<nome>.kube.sandmux.internal`, a padrão pelo `current-context`, e nome repetido recusado. Teste de unidade no `daemon/vault.rs`

## 2. Comando

- [x] 2.1 `sandmux vault add-cloud kubernetes [--context <c>]… [--copy]`, sem o `<nome>` (o nome vem do contexto): lê os contextos pelo `kubectl` do host (`SANDMUX_KUBECTL`, com o `KUBECONFIG` do usuário), guarda um por contexto e pula, com o aviso, os nomes repetidos; recusa um `--context` ausente, o `kubectl` fora do host, o `auth-provider` e o usuário e senha, e, com `--copy`, o plugin `exec`. A mensagem lista as identidades guardadas e avisa do RBAC. Testes em `tests/cloud.rs` com um `kubectl` falso: todos os contextos, um contexto, a cópia, e as recusas da spec
- [x] 2.2 `sandmux vault list` mostra `kube:<nome>  cloud  kubernetes`, a fonte, o contexto, o servidor e a marca de padrão, sem token, certificado nem chave; `vault set` e a troca de domínios recusam. Teste em `tests/cloud.rs`, que confere que nenhuma parte da chave copiada aparece

## 3. O real fora do sandbox

- [x] 3.1 Resolução do real: o contexto lido pelo `kubectl config view --raw --flatten --minify -o json` (referência) ou do segredo (cópia); o token, o `tokenFile` lido no host, o certificado de cliente; e o plugin `exec` rodado no host (o `env` do kubeconfig, `KUBERNETES_EXEC_INFO` com `interactive: false`, o stdin vazio, 60 s), com a saída `ExecCredential` lida. Testes de unidade: a leitura da visão do `kubectl`, a do `ExecCredential` (com e sem validade, com certificado) e o motivo de uma falha do plugin com a última linha do stderr
- [x] 3.2 O cache guarda o `Real` inteiro (com o destino) e zera o token e a chave no fim; sem validade, o real vale por 15 minutos. Teste de unidade com um relógio controlado
- [x] 3.3 O `sandmux doctor` avisa de cada referência do Kubernetes sem o `kubectl` no host. Teste do `doctor` sem o `kubectl` no `PATH`

## 4. A aba

- [x] 4.1 Em `sandmux-policy`, o kubeconfig da aba (um contexto por identidade com o nome original e o namespace, o servidor sintético, a CA em `~/.kube/sandmux-ca.crt`, o token falso, o contexto atual na padrão), gravado pelo daemon em `runtime_dir/kube/<tag>` e apagado no fim. O `compose` liga a pasta com escrita em `~/.kube`, põe a CA do lançamento no arquivo e define `KUBECONFIG`. Testes de unidade do arquivo gerado, e teste em `tests/cloud.rs` que lê o kubeconfig dentro da aba e roda `kubectl config use-context` com o `kubectl` falso

## 5. O gateway

- [x] 5.1 O DNS responde `*.kube.sandmux.internal` com `10.0.2.253`; o TCP a esse IP toma o caminho do Kubernetes antes da política, e fecha, com o motivo, o que não é TLS com um SNI sintético. Teste de unidade do DNS e da escolha do caminho
- [x] 5.2 O caminho do Kubernetes: as injeções pedidas antes de conectar; o cliente TLS com a CA do cluster (ou as do sistema), o certificado de cliente, o `insecure` e o `server_name` (inclusive IP); a conexão ao servidor guardado (o `127.0.0.1` do host); a troca ou a retirada do `Authorization`, o `Host` reescrito e o nome da identidade e o servidor real na linha `https`. Testes em `tests/cloud.rs` com o API server local: com token (o servidor recebe o real, o sandbox só o falso), com certificado de cliente exigido (o servidor recebe o certificado e nenhum `Authorization`), e com o servidor no `127.0.0.1` do host
- [x] 5.3 Sem o real: a resposta local com `401` (ou `404` sem a identidade) e um `Status` do Kubernetes com o motivo, sem ir ao destino, e o aviso único do daemon. Teste de unidade da resposta, e teste em `tests/cloud.rs` com o plugin `exec` falso falhando
- [x] 5.4 Os upgrades que não são WebSocket passam como túnel depois do `101`, com a linha `https` e os bytes. Teste de unidade no `mitm.rs` com um destino que responde `101` e ecoa os bytes

## 6. TUI

- [x] 6.1 A seção Cofre e o grafo mostram as identidades de Kubernetes no domínio sintético delas, com a marca de cloud e os detalhes (`cloud · kubernetes`, fonte, contexto, servidor, padrão, falso); `x` apaga, `v` e `d` recusam. Teste em `tests/tui.rs`, que confere que nenhuma parte do certificado aparece

## 7. Documentação e fechamento

- [x] 7.1 `docs/cloud.md` e `docs/en/cloud.md` com a seção do Kubernetes (o comando, as fontes, o kubeconfig da aba, o nome sintético, os plugins `exec`, o aviso do RBAC, os limites); o `docs/vault.md`, o README e o protocolo 47 em `docs/sessions.md`, nos dois idiomas, com o teste de docs atualizado
- [x] 7.2 Rodar o `./scripts/check.sh` inteiro, sob o vigia, e conferir que termina verde e sem processo de teste vivo
- [x] 7.3 Atualizar o roadmap: a situação do change 94, as decisões na seção 2, os limites na seção 6, e a verificação à mão com um cluster real

## Workflow follow-up

- Commit e push num branch novo, PR e merge, com o change arquivado no mesmo PR; release nova.
- Verificação à mão: um `kubectl get ns` numa aba contra um cluster kind do host, numa HOME de teste.
