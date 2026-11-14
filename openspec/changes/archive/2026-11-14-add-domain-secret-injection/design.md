# Design

## Context

- **Change 13:**
  - cada segredo tem um valor falso;
  - o gateway pede os segredos ao daemon a cada conexão decifrada e troca o falso pelo real nos cabeçalhos dos pedidos ao dono;
  - os presets do opencode e do hermes recebem os falsos dos provedores de inferência (`RunOptions.fake_env`).
- **Change 12:** os domínios donos dos segredos já entram na lista de decifração das sessões.
- **Docker privado (changes 24 e 10):** o gateway do Docker roda no daemon, sem CA, sem decifração e sem acesso ao cofre. A visão do Docker tem um `/etc/ssl/certs` do sistema.
- **HOME sintético:** guarda os arquivos de configuração das ferramentas entre execuções. O hermes já recebe um `.env` só daquela execução por `Mount::File`.

## Goals / Non-Goals

**Goals:**

- `gh`, `git push` por HTTPS, `npm install` de um registry privado, `pip install` de um índice privado, `cargo publish` e `docker pull` de um registry privado, sem token real no sandbox.

**Non-Goals:**

- OAuth e renovação de tokens.
- Registries com autenticação fora de cabeçalhos (certificado de cliente).
- Ferramentas além das seis da lista: um segredo sem ferramenta continua sendo trocado no tráfego, e o usuário põe o falso onde quiser.

## Decisions

### 1. Ferramentas no segredo

- **No cofre:** `Secret` ganha `tools: Vec<String>` (padrão vazio).
- **No `VaultAdd`:** aceita `tools`. Sem nenhuma, o daemon deriva pelos domínios conhecidos (`presets::KNOWN_TOOLS`). No desbloqueio, os segredos antigos ganham as ferramentas pelos domínios.
- **Ferramentas válidas:** `gh`, `git`, `npm`, `pip`, `cargo` e `docker`.

### 2. Valor falso nas ferramentas

- **Quem calcula:** o daemon, para cada lançamento com o cofre desbloqueado, gera a lista `ToolSecret { tool, domain, fake }` (um por domínio exato do segredo; curingas não servem às ferramentas e são pulados) e a passa ao `prepare` (`RunOptions.tool_secrets`).
- **Na composição:**
  - **`gh`:** `GH_TOKEN=<falso>` (o primeiro segredo de `gh`).
  - **`git`:** o ajudante de credenciais vai por variáveis, sem arquivo:
    - `GIT_CONFIG_COUNT=n`;
    - para cada domínio, `GIT_CONFIG_KEY_i=credential.https://<domínio>.helper` e `GIT_CONFIG_VALUE_i=!f() { echo username=sandmux; echo password=<falso>; }; f`.
  - **`npm`:** `Mount::File` em `~/.npmrc`, com o `.npmrc` guardado no HOME sintético mais `//<domínio>/:_authToken=<falso>`.
  - **`pip`:** `PIP_EXTRA_INDEX_URL=https://sandmux:<falso>@<domínio>/simple` (vários separados por espaço), fora do `pypi.org`.
  - **`cargo`:** `CARGO_REGISTRY_TOKEN=<falso>` para `crates.io`.
  - **`docker`:** `Mount::File` em `~/.docker/config.json`, com o JSON guardado (se existir) mais `auths[<domínio>].auth = base64("sandmux:<falso>")`.
- **Sem cofre** (`run` sem terminal, cofre trancado): nada disso entra; as ferramentas usam o que estiver no HOME sintético.

### 3. Troca dentro do `Basic`

`rewrite_headers`, num valor que começa com `Basic `:

1. decodifica o base64;
2. troca o falso pelo real;
3. codifica de novo.

O resto continua igual.

### 4. Docker privado com troca

- **No daemon, ao subir o gateway do Docker:**
  - gera uma CA (como o `prepare`);
  - põe na política dele a lista de decifração com os domínios donos dos segredos `docker` (somados no `with_vault` só dos segredos `docker`);
  - dá a ele o acesso ao cofre (o socket e a etiqueta `docker-<id>`).
- **Na visão do Docker:** o pacote de CAs do sistema mais a CA cobre o pacote do sistema, como no sandbox.
- **Confiança do `dockerd`:** ele usa o pacote do sistema para os registries, então passa a confiar no gateway.
- **Desbloqueio depois da subida:** a CA do Docker nasce com o Docker da pasta. Um desbloqueio do cofre depois disso entra pela releitura da política e pelo `VaultInject` a cada conexão.

## Risks / Trade-offs

- **[`docker login` grava o falso]** O `docker login` dentro do sandbox grava o falso no `config.json` só daquela execução, que some no fim. → É o comportamento desejado; o real nunca entra.
- **[Ajudante do git por variável]** Um `.gitconfig` do repositório com outro `credential.helper` pode vir antes. → As variáveis `GIT_CONFIG_*` têm precedência de linha de comando no git, acima dos arquivos.
- **[Curingas]** Um segredo com `*.empresa.com` não ganha configuração de ferramenta (não há host para escrever). → A troca no tráfego vale; a documentação orienta a usar o domínio exato.

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart` (protocolo 12). Os segredos existentes ganham as ferramentas dos domínios conhecidos no primeiro desbloqueio.
