## ADDED Requirements

### Requirement: Destino de uma identidade de Kubernetes
O destino de uma identidade de Kubernetes MUST ser verificado pela CA do cluster guardada com ela (ou pelas CAs do sistema, quando ela não tem uma), pelo nome do servidor dela, e MUST receber o certificado de cliente dela, quando há. A verificação só MUST ser pulada quando o kubeconfig do host pede `insecure-skip-tls-verify`.

#### Scenario: API server com a CA do cluster
- **WHEN** o API server de `kube:kind-dev` apresenta um certificado assinado pela CA do cluster, que as CAs do sistema não conhecem
- **THEN** o gateway aceita o certificado, e o pedido segue

### Requirement: Outros upgrades decifrados
Numa conexão decifrada, um pedido de upgrade que não é WebSocket (o SPDY do `kubectl exec` e do `port-forward`, por exemplo) MUST ser repassado ao destino com os cabeçalhos dele. Com a resposta `101`, o gateway MUST passar os bytes nos dois sentidos, como um túnel, até uma das pontas fechar, e MUST gravar uma linha `https` com o método, o caminho, o status `101` e os bytes ao fim.

#### Scenario: kubectl exec por SPDY
- **WHEN** o agente roda `kubectl exec` e o `kubectl` pede o upgrade `SPDY/3.1`
- **THEN** o terminal do contêiner funciona pela conexão, e o registro tem uma linha `https` com o status `101`
