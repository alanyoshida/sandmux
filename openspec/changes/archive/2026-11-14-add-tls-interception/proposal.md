# Proposal

## Why

A troca de chaves no tráfego (changes 13 e 25) precisa ler e reescrever os cabeçalhos HTTP que o agente manda: a chave falsa vira a real só quando o destino é o dono dela. Hoje o gateway só vê o SNI do TLS; o conteúdo vai cifrado de ponta a ponta. O roadmap decidiu decifrar o TLS só nos domínios com segredo a injetar, ampliável por perfil, com uma CA local em que o sandbox confia, e registrar o método e o caminho do tráfego decifrado.

## What Changes

- **CA efêmera por lançamento:**
  - cada lançamento com rede gera uma CA nova (chave ECDSA P-256);
  - o certificado da CA entra no sandbox;
  - a chave vai ao gateway pela especificação, num memfd, e nunca toca o disco;
  - a CA some quando o sandbox termina, e um sandbox não confia na CA de outro.
- **Confiança injetada no sandbox:**
  - um arquivo com as CAs do sistema mais a do lançamento cobre o pacote de CAs do sistema (`/etc/ssl/certs/ca-certificates.crt` e os caminhos equivalentes de outras distros que existirem);
  - as variáveis `SSL_CERT_FILE`, `NODE_EXTRA_CA_CERTS`, `REQUESTS_CA_BUNDLE`, `CURL_CA_BUNDLE` e `GIT_SSL_CAINFO` apontam para ele.
- **Decifração só onde é preciso:**
  - o gateway decifra uma conexão TLS quando o SNI casa com a lista de decifração;
  - a lista junta os domínios donos dos segredos do cofre desbloqueado e a chave nova `decrypt` da configuração (e `--decrypt` no `run`);
  - o resto passa cifrado de ponta a ponta, como hoje;
  - a lista vai na política de rede e muda na hora, como as regras do change 9.
- **Proxy HTTP nas conexões decifradas:**
  - o gateway termina o TLS com um certificado do domínio assinado pela CA do lançamento, oferecendo só HTTP/1.1 (ALPN);
  - abre TLS até o destino real, verificando o certificado dele pelas CAs do sistema;
  - repassa cada pedido HTTP. É nesse ponto que os changes 13 e 25 trocam a chave.
  - Um certificado inválido do destino bloqueia a conexão, com o motivo no registro.
- **Registro de método e caminho:** cada pedido decifrado vira uma linha do registro de rede (`proto` `https`), com o método, o caminho sem a query, o status da resposta e os bytes. O corpo e os cabeçalhos nunca são gravados.
- **Painel e `sandmux net`:** as linhas decifradas mostram o método e o caminho.

## Capabilities

### New Capabilities

- `tls-interception`: CA por lançamento, confiança no sandbox, lista de decifração, terminação e reabertura do TLS, verificação do destino, registro de método e caminho.

### Modified Capabilities

- `network-gateway`: "Registro de rede" ganha os campos do tráfego decifrado.
- `config-profiles`: "Chaves de ajuste" ganha `decrypt`.

## Impact

- **`sandmux-net`:**
  - geração da CA e dos certificados (`rcgen`);
  - TLS (`rustls`, `tokio-rustls`, `rustls-native-certs`);
  - proxy HTTP/1.1 (`hyper`, `hyper-util`, `http-body-util`);
  - linha de registro com método, caminho e status.
- **`sandmux-spec`:** a seção `network` ganha a CA; a política ganha a lista de decifração.
- **`sandmux-policy`:**
  - chave `decrypt` nas camadas;
  - a composição monta o pacote de CAs e as variáveis;
  - a política junta os domínios dos segredos.
- **`sandmux`:**
  - o daemon junta à política os domínios donos dos segredos do cofre;
  - `--decrypt` no `run`;
  - o painel e o `sandmux net` mostram método e caminho.
- **Documentação:** `docs/network.md`, `docs/config.md`, `docs/vault.md` e o README.
- **Migração:** `sandmux daemon restart`. Sem segredos com domínio e sem `decrypt`, nada é decifrado.
