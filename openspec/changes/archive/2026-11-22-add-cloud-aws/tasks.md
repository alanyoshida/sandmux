Os testes rodam no host, sob o vigia de memória. Nenhum teste fala com a AWS:
- o `aws` do host é um script falso (`SANDMUX_AWS`);
- o domínio da AWS vira `host.sandmux.internal` (`SANDMUX_TEST_AWS_DOMAIN`);
- um servidor HTTPS local confere a assinatura com a chave real;
- na aba, um cliente SigV4 mínimo em Python assina com o par falso.

## 1. Protocolo e cofre

- [x] 1.1 Em `sandmux-proto`, `AwsCreds` (ID, chave, token de sessão, token falso; o `Debug` sem a chave nem os tokens) em `Injection.aws`; `region`, `secret_fake` e `env` em `CloudInfo`; `Msg::CloudCapture` e `Msg::CloudCaptured`; protocolo 48. Teste de unidade: uma injeção do protocolo 47 ainda lê, e o `Debug` não mostra a chave
- [x] 1.2 Em `sandmux-spec`, o domínio da AWS (`*.amazonaws.com`, com `SANDMUX_TEST_AWS_DOMAIN`), os prefixos `AKIASANDMUX` e `ASIASANDMUX` e o gerador dos falsos (o ID de 20 caracteres e a chave de 40). Teste de unidade do formato
- [x] 1.3 No daemon, `add_cloud` aceita o provedor `aws`: valida o nome e a cópia (as duas partes da chave), põe os falsos e o domínio, e escolhe a padrão. Teste de unidade no `daemon/vault.rs`, que confere que a lista não tem a chave

## 2. Comando

- [x] 2.1 A leitura dos perfis do host (INI do `config` e do `credentials`, com `AWS_CONFIG_FILE` e `AWS_SHARED_CREDENTIALS_FILE`): os nomes, a região, a chave estática e o que impede a cópia (SSO, `role_arn`, `credential_process`, token de sessão). Testes de unidade com perfis de SSO, de `assume-role` e de chave
- [x] 2.2 `sandmux vault add-cloud aws [--profile <p>]… [--copy]`:
  - guarda um por perfil, com a padrão pelo `default`, pelo `AWS_PROFILE` ou pela primeira;
  - pula, com o aviso, os nomes repetidos e, sem `--profile`, os perfis recusados;
  - recusa o perfil ausente e, com `--copy`, o perfil sem chave estática;
  - a mensagem lista as identidades e avisa do IAM;
  - o `vault list` mostra a fonte, o perfil, a região e o ID falso, sem a chave.

  Testes em `tests/aws.rs`: todos os perfis, um perfil, a cópia e as recusas da spec
- [x] 2.3 O `sandmux doctor` avisa de cada referência da AWS sem o `aws` no host. Teste com `SANDMUX_AWS` apontando para um caminho que não existe

## 3. O real fora do sandbox

- [x] 3.1 A resolução:
  - na referência, pelo `aws configure export-credentials --profile <p> --format process`, com as variáveis do perfil, `AWS_PAGER=` e 60 s de limite;
  - na cópia, do cofre;
  - a validade lida em RFC 3339 com fuso;
  - sem validade, 15 minutos.

  Testes de unidade: a saída do `export-credentials` (com e sem token e validade), o motivo de uma falha (a última linha do erro) e a data com `+00:00`
- [x] 3.2 As credenciais capturadas no daemon: guardadas só na memória com falsos novos, até a validade; incluídas nas injeções do domínio da AWS; apagadas quando vencem e quando o cofre é trancado. Teste de unidade do armazenamento com um relógio controlado

## 4. A aba

- [x] 4.1 Em `sandmux-policy`, os arquivos da pasta `~/.aws` da aba (`config` com `[default]` e `[profile <nome>]` e a região; `credentials` com os pares falsos; o `default` com a padrão), gravados pelo daemon em `runtime_dir/aws/<tag>` e apagados no fim. O `compose` liga a pasta com escrita e define `AWS_CONFIG_FILE` e `AWS_SHARED_CREDENTIALS_FILE`, e `AWS_CA_BUNDLE` entra em `CA_VARS`. Testes de unidade dos arquivos, e teste em `tests/aws.rs` que os lê dentro da aba e confere que o HOME sintético não muda

## 5. O gateway

- [x] 5.1 `sandmux-net/src/sigv4.rs`: o pedido canônico (o caminho do S3 e dos outros, a query, os cabeçalhos assinados, o hash do corpo), a chave derivada, a assinatura e a URL pré-assinada. Testes de unidade com os exemplos da AWS (`get-vanilla`, `GET Object` do S3 com `Range`) e com query e caminho que pedem escape
- [x] 5.2 A reassinatura no `mitm`:
  - a identidade achada pelo ID falso, no `Authorization` ou no `X-Amz-Credential`;
  - o `X-Amz-Security-Token` posto, trocado ou tirado;
  - o corpo lido só sem o `x-amz-content-sha256`, até 32 MiB, e `413` acima disso;
  - o nome da identidade na linha `https`;
  - sem o real, `403` no formato da AWS, em XML ou JSON.

  Testes de unidade no `mitm.rs`, e teste em `tests/aws.rs` com o servidor local que confere a assinatura: com o cabeçalho, com a URL pré-assinada, com o token de sessão de uma referência, e o `403` com o `aws` falso falhando
- [x] 5.3 O `aws-chunked` assinado (`STREAMING-AWS4-HMAC-SHA256-PAYLOAD` e `-PAYLOAD-TRAILER`): cada assinatura de pedaço e a do trailer refeitas em fluxo, sem mudar o tamanho. Testes de unidade com o exemplo da AWS (64 KiB de `a`: a semente e o primeiro pedaço) e com pedaços divididos em leituras de tamanhos variados
- [x] 5.4 As credenciais emitidas:
  - reconhecer o pedido pelo `Action` do STS, pelo `X-Amz-Target` ou pelo caminho do SSO;
  - ler a resposta (XML ou JSON) até 1 MiB, sem compressão;
  - trocar o real pelos falsos que o daemon devolve;
  - recusar com `502` quando não dá.

  Testes de unidade da leitura das respostas do STS, do SSO e do Cognito, e teste em `tests/aws.rs`: o `GetSessionToken` na aba devolve falsos, e um pedido com eles chega ao servidor assinado com o real capturado

## 6. TUI

- [x] 6.1 A seção Cofre e o grafo mostram as identidades da AWS no domínio `*.amazonaws.com`, com a marca de cloud e os detalhes (`cloud · aws`, fonte, perfil, região, padrão, ID falso). Teste em `tests/tui.rs`, que confere que nenhuma parte da chave copiada aparece

## 7. Documentação e fechamento

- [x] 7.1 `docs/cloud.md` e `docs/en/cloud.md` com a seção da AWS:
  - o comando e as fontes;
  - a pasta da aba e a reassinatura;
  - as credenciais emitidas;
  - o aviso do IAM (`iam:CreateAccessKey`, Secrets Manager);
  - os limites (SigV4a, event streams no pedido, China).

  Também o `docs/vault.md`, o README e o protocolo 48 em `docs/sessions.md`, nos dois idiomas, com o teste de docs atualizado
- [x] 7.2 Rodar o `./scripts/check.sh` inteiro, sob o vigia, e conferir que termina verde e sem processo de teste vivo
- [x] 7.3 Atualizar o roadmap: a situação do change 95, as decisões na seção 2, os limites na seção 6, o fim da etapa 19 e a verificação à mão com uma conta da AWS

## Workflow follow-up

- Commit e push num branch novo, PR e merge, com o change arquivado no mesmo PR; release nova.
- Verificação à mão pelo usuário, numa máquina com o `aws` logado: `aws s3 ls`, `aws sts get-caller-identity` e `aws sts assume-role` dentro de uma aba, com a identidade por referência (SSO) e por cópia.
