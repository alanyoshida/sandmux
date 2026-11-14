# Proposal

## Why

O change 13 tirou do sandbox as chaves de inferência: o agente recebe um valor falso, e o gateway o troca pelo real no pedido ao dono. As ferramentas de desenvolvimento ainda precisam de tokens reais lá dentro: o `gh` (GitHub), o git por HTTPS, e os registries privados de npm, pip, cargo e de imagens de contêiner. O roadmap decidiu o segredo genérico por domínio: qualquer token do cofre ligado a um domínio é trocado no tráfego, e o valor falso é gravado na configuração da ferramenta.

## What Changes

- **Ferramentas de um segredo:**
  - um segredo do cofre ganha a lista de ferramentas que o usam: `gh`, `git`, `npm`, `pip`, `cargo` e `docker` (opção `--tool` do `sandmux vault add`);
  - sem `--tool`, os domínios conhecidos decidem: `github.com` leva a `gh` e `git`; `registry.npmjs.org` a `npm`; `pypi.org` a `pip`; `crates.io` a `cargo`; `ghcr.io`, `registry-1.docker.io` e `docker.io` a `docker`.
- **Valor falso na configuração da ferramenta**, em toda sessão com o cofre desbloqueado, sem tocar os arquivos guardados no HOME sintético:
  - **`gh`:** `GH_TOKEN` com o falso;
  - **`git`:** um ajudante de credenciais para `https://<domínio>`, que responde com o falso, pelas variáveis `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_n` e `GIT_CONFIG_VALUE_n`;
  - **`npm`:** um `~/.npmrc` só daquela execução, com o conteúdo guardado mais `//<domínio>/:_authToken=<falso>`;
  - **`pip`:** `PIP_EXTRA_INDEX_URL` com o usuário e o falso no índice (nos domínios que não são `pypi.org`);
  - **`cargo`:** `CARGO_REGISTRY_TOKEN` para `crates.io`;
  - **`docker`:** um `~/.docker/config.json` só daquela execução, com `auths` do domínio.
- **Troca dentro do `Authorization: Basic`:** a troca do change 13 passa a abrir o `Basic` (base64 de `usuário:senha`), trocar o falso e fechar de novo. Assim valem o git por HTTPS, o pip e o login dos registries de contêiner.
- **Docker privado com troca:** o gateway do Docker de cada pasta passa a ter a CA dele, a lista de decifração com os domínios donos dos segredos `docker`, e o acesso ao daemon para os segredos. O pacote de CAs da visão do Docker ganha a CA, para o `dockerd` confiar. O `docker login` e o `docker pull` de um registry privado funcionam com o falso.

## Capabilities

### New Capabilities

- `tool-secrets`: ferramentas de um segredo, os domínios conhecidos e o valor falso na configuração de cada ferramenta.

### Modified Capabilities

- `key-injection`: "Troca no tráfego só para o dono" passa a valer dentro do `Authorization: Basic` e no gateway do Docker privado.
- `secret-vault`: "Cadastro de segredos" ganha `--tool`.

## Impact

- **`sandmux-vault`:** campo `tools` no segredo.
- **`sandmux-policy`:** tabela de ferramentas e domínios conhecidos; a composição põe as variáveis e os arquivos só daquela execução.
- **`sandmux-net`:** troca no `Basic`.
- **`sandmux`:**
  - o daemon passa as ferramentas e os falsos a cada lançamento;
  - o gateway do Docker ganha a CA, a decifração e o acesso ao cofre, e a visão do Docker o pacote de CAs;
  - `--tool` no `sandmux vault add`;
  - a coluna de ferramentas no `vault list`.
- **`sandmux-proto`:** `VaultAdd` e `VaultEntry` ganham `tools`; `PROTOCOL` 12.
- **Documentação:** `docs/vault.md`, `docs/docker.md` e o README.
- **Migração:** `sandmux daemon restart` (protocolo 12). Os segredos existentes ganham as ferramentas dos domínios conhecidos.
