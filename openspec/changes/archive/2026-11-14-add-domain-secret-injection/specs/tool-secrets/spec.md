# Spec Delta

## Purpose
Define como os segredos do cofre chegam às ferramentas de desenvolvimento dentro do sandbox: as ferramentas de cada segredo, os domínios conhecidos e o valor falso gravado na configuração de cada ferramenta só durante a execução.

## ADDED Requirements

### Requirement: Ferramentas de um segredo
Um segredo do cofre MUST poder ter uma lista de ferramentas, dadas por `sandmux vault add --tool <ferramenta>` (repetível): `gh`, `git`, `npm`, `pip`, `cargo` e `docker`. Sem `--tool`, a lista MUST vir dos domínios conhecidos do cenário "Domínios conhecidos". Uma ferramenta desconhecida MUST ser recusada.

#### Scenario: Domínios conhecidos
- **WHEN** o usuário guarda um segredo com o domínio `github.com`, `registry.npmjs.org`, `pypi.org`, `crates.io`, `ghcr.io`, `registry-1.docker.io` ou `docker.io`, sem `--tool`
- **THEN** o segredo fica com as ferramentas `gh` e `git` (`github.com`), `npm` (`registry.npmjs.org`), `pip` (`pypi.org`), `cargo` (`crates.io`) ou `docker` (os três últimos)

#### Scenario: Registry privado
- **WHEN** o usuário roda `sandmux vault add npm-empresa --domain npm.empresa.com --tool npm`
- **THEN** o segredo fica com a ferramenta `npm`

### Requirement: Valor falso na configuração da ferramenta
Em toda sessão com o cofre desbloqueado, cada segredo com ferramentas MUST chegar a elas com o valor falso, pela forma de cada ferramenta do cenário "Forma de cada ferramenta", sem alterar os arquivos guardados no HOME sintético.

#### Scenario: Forma de cada ferramenta
- **WHEN** o cofre tem um segredo para cada ferramenta e uma sessão sobe
- **THEN** o `gh` recebe `GH_TOKEN`; o `git`, um ajudante de credenciais para `https://<domínio>` pelas variáveis `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_n` e `GIT_CONFIG_VALUE_n`; o `npm`, um `~/.npmrc` só daquela execução com a linha do token do domínio; o `pip`, `PIP_EXTRA_INDEX_URL` com o usuário e o falso (fora do `pypi.org`); o `cargo`, `CARGO_REGISTRY_TOKEN`; e o `docker`, um `~/.docker/config.json` só daquela execução com `auths` do domínio

#### Scenario: gh no sandbox
- **WHEN** o cofre desbloqueado tem um segredo de `github.com` e o agente roda `gh auth status`
- **THEN** o `gh` vê um token, que é o valor falso, e o valor real não está em nenhum arquivo nem variável do sandbox
