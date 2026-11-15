## 1. Busca

- [x] 1.1 `Replacement.real` com `Debug` que o esconde; `find_real_secrets` e a gravação do texto com os falsos e do manifesto, com testes

## 2. Diagnóstico

- [x] 2.1 Seção do `sandmux doctor` com as chaves reais, sem valores
- [x] 2.2 `sandmux doctor --fix`: confirmação por arquivo, cofre ausente ou trancado, domínio pelo provedor ou perguntado, `VaultAdd` antes da regravação

## 3. Verificação e documentação

- [x] 3.1 Teste de integração: chave real no `auth.json` do HOME sintético, o `doctor` lista sem o valor, o `--fix` leva ao cofre com o domínio e deixa o falso no arquivo
- [x] 3.2 `docs/vault.md`, README
- [x] 3.3 O falso depois de um esquema (`Bearer sandmux-fake-…`) não é tomado por chave real (achado no teste: a busca acusava o falso que ela mesma tinha posto)
- [x] 3.4 Só arquivos de configuração (JSON, YAML, `.env`) na busca, e `*_fingerprint`, `*_hash` e `*_digest` fora das chaves de credencial (achados no HOME real: tokens de exemplo em skills e o `secret_fingerprint` do hermes); o domínio de uma chave do `auth.json` do opencode pelo `baseURL` do provedor no `opencode.json`
