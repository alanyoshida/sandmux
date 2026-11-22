## MODIFIED Requirements

### Requirement: Confiança no sandbox
O sandbox MUST confiar na CA do lançamento além das CAs do sistema: o pacote de CAs do sistema MUST ser coberto por um arquivo com as duas, e as variáveis `SSL_CERT_FILE`, `NODE_EXTRA_CA_CERTS`, `REQUESTS_CA_BUNDLE`, `CURL_CA_BUNDLE`, `GIT_SSL_CAINFO` e `CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE` MUST apontar para ele.

#### Scenario: curl confia no gateway
- **WHEN** um domínio está na lista de decifração e o agente roda `curl https://<domínio>/`
- **THEN** o `curl` aceita o certificado apresentado pelo gateway, sem opção extra

#### Scenario: gcloud confia no gateway
- **WHEN** `*.googleapis.com` está na lista de decifração e o agente roda um comando do `gcloud`
- **THEN** o `gcloud` aceita o certificado apresentado pelo gateway, sem opção extra
