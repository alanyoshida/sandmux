## Context

- `docs/` tem 21 arquivos em português e os 21 pares em `docs/en/`, sem front matter. O título é o primeiro `# ` do arquivo. Os links entre eles são relativos (`network.md`, `vault.md#grafo-do-cofre`), e cada doc em inglês aponta para o português com `../<nome>.md`. Não há imagens nem HTML solto nos docs.
- `crates/sandmux/tests/docs.rs` (`documentacao_em_ingles_acompanha_a_portuguesa`) exige o par em inglês, valida os exemplos de TOML em inglês e confere que todo link relativo aponta para um arquivo que existe. Ele roda no `scripts/check.sh`.
- O único workflow é o `release.yml`, que roda numa tag. Os testes rodam na máquina de desenvolvimento e numa VM, não no GitHub Actions.
- O repositório não tem Node, Ruby nem gerador de site. O VHS (v0.12.1) está instalado na máquina de desenvolvimento; o Hugo, não.
- Na máquina de desenvolvimento o auxiliar root está instalado (`/run/sandmux-helper.sock`). O cliente sobe o daemon pelo systemd de usuário, salvo com `SANDMUX_DAEMON_START=direct`. Os caminhos do daemon seguem `XDG_RUNTIME_DIR`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME` e `XDG_STATE_HOME`, e o socket do auxiliar segue `SANDMUX_HELPER_SOCKET`.

## Goals / Non-Goals

**Goals:**

- Os 21 docs (23, com os dois novos) viram páginas sem mudar o texto deles, e continuam bons de ler no GitHub.
- Um link quebrado entre docs falha o build, no pull request.
- O site inteiro sai de `website/` com o Hugo, sem Node, sem Sass e sem tema de terceiros.
- Regenerar os screenshots é um comando, e ele não encosta nas sessões, no cofre, no daemon nem no auxiliar root de quem roda.

**Non-Goals:**

- Busca no site. Pode vir depois, com um índice gerado pelo próprio Hugo, sem serviço externo.
- Versões do site por release: o site mostra sempre `main`.
- Domínio próprio, analytics, comentários.
- Rodar o sandmux no CI, para os screenshots ou para qualquer outra coisa.
- Reescrever os docs existentes. Só os dois novos são escritos aqui.

## Decisions

### 1. Hugo com os docs montados, não copiados

`website/hugo.toml` declara os mounts do módulo principal:

```toml
[[module.mounts]]
source = "content"
target = "content"

[[module.mounts]]
source = "../docs"
target = "content/docs"
lang = "pt"
excludeFiles = ["en/**"]

[[module.mounts]]
source = "../docs/en"
target = "content/docs"
lang = "en"
```

Além dos mounts de `layouts`, `static`, `data`, `i18n` e `assets`, que voltam a ser declarados porque um mount explícito substitui os padrões. Como os dois idiomas montam o mesmo caminho (`content/docs/network.md`), o Hugo liga as traduções sozinho, e a troca de idioma acha o par.

- `defaultContentLanguage = "pt"` e `defaultContentLanguageInSubdir = false`: o português na raiz e o inglês em `/en/`, como o `README.md` e o `README.en.md`.
- `baseURL = "https://alanyoshida.github.io/sandmux/"`.
- `markup.goldmark.parser.autoHeadingIDType = "github"`: as âncoras saem iguais às do GitHub, com acento (`#decifração-do-tls`), e os links que já funcionam lá funcionam no site.
- `disableKinds = ["taxonomy", "term", "rss"]`.

Alternativas: copiar os docs para `website/content/` num passo do build (não fica commitado, mas some o vínculo das traduções por caminho e o servidor local do Hugo não vê a edição na hora); manter uma cópia própria (recusada pelo usuário).

### 2. Título e menu sem front matter

Os docs não ganham front matter, para não poluir a leitura no GitHub. Um partial `doc-title` tira o título do primeiro `^# (.+)$` do `.RawContent`; ele é usado no `<title>`, no menu e na troca de idioma. O layout não imprime outro `<h1>`: o do próprio arquivo já aparece.

O menu fica em `website/data/nav.toml`, uma lista de seções, cada uma com uma chave de tradução e a lista de docs, pelo nome do arquivo sem `.md`. O rótulo de cada item é o título do doc no idioma da página. Proposta de seções:

| Seção | Docs |
|---|---|
| Começar | install, tui, sessions, config |
| Arquitetura e segurança | architecture, security, helper, launch-spec |
| Sandbox | files, network, limits, docker, window |
| Segredos | vault, git, cloud, database, mcp |
| Harnesses | claude, hermes, omp, agy |
| Assistente | chat |

Os nomes das seções e o resto do texto da interface ficam em `website/i18n/pt.toml` e `en.toml`. O teste do projeto exige que todo doc esteja no menu e que todo item do menu exista (decisão 7).

### 3. Links e imagens por render hooks

Um render hook de link (`layouts/_markup/render-link.html`) resolve o destino pelo caminho do arquivo no repositório. A pasta de origem é `docs/` para um doc em português, `docs/en/` para um em inglês e `website/content/` para as páginas do site. O caminho resolvido decide:

| Caminho resolvido | Destino no site |
|---|---|
| `docs/<nome>.md` | a página `docs/<nome>/` em português |
| `docs/en/<nome>.md` | a página `en/docs/<nome>/` |
| `README.md`, `README.en.md` | a página inicial no idioma do README |
| qualquer outro arquivo do repositório | `https://github.com/alanyoshida/sandmux/blob/main/<caminho>` |
| URL com esquema, `#âncora`, `mailto:` | sem mudança |

A âncora é mantida. Um `.md` em `docs/` que não existe chama `errorf`, e o build para com o nome da página e o link. Com a âncora, o hook confere `Fragments.Identifiers` da página de destino e também chama `errorf` quando ela falta.

Um render hook de imagem faz o mesmo para `website/static/<caminho>`, levado a `/sandmux/<caminho>`. Assim um doc pode mostrar um screenshot com `![...](../website/static/img/tui.png)`, que aparece no GitHub e no site.

### 4. Layouts próprios, sem JavaScript

O tema fica em `website/layouts/`, com um CSS só, em `website/assets/`, passado por `resources.Minify` e `Fingerprint`, o que o Hugo padrão faz sem a edição extended.

- A página inicial tem a apresentação, a galeria de screenshots, o comando de instalação, o resumo das camadas e os links.
- As páginas de doc têm o menu lateral, o conteúdo e um índice da página (`.TableOfContents`).
- A troca de idioma fica no topo.
- No celular, o menu abre num `<details>`.
- Claro e escuro seguem `prefers-color-scheme`.
- Fontes servidas pelo próprio site, em `website/static/fonts/`, cortadas para o alfabeto latino em woff2: Clear Sans (Apache 2.0) no texto e nos títulos, Fira Code (OFL, sem nome reservado) no código e no terminal, com as licenças ao lado. Nenhuma vem de outro endereço. Trocou as fontes do sistema na implementação (2026-11-27), pela regra da skill de design de não usar a fonte do sistema como voz do título. O realce de código vem do Chroma, com as classes e o CSS gerados no próprio site.
- Visual: o padrão das páginas de ferramenta para desenvolvedores, com a régua do Ghostty e do Zed (escolha do usuário, 2026-11-27); logo "terminal na caixa", com o destaque em cor de areia.

Nada é carregado de outro endereço, o que combina com o sandmux, que nunca faz chamada de rede por conta própria.

Alternativas: um tema pronto (Docsy, hugo-book), que traria Node ou Sass, submódulo e um visual genérico; o mdBook, recusado pelo usuário em favor do Hugo.

### 5. A página inicial e os screenshots

O conteúdo da página inicial fica em `website/content/_index.md` e `_index.en.md`, com o título e a descrição no front matter. A galeria vem de `website/data/screens.toml`: arquivo, tipo e legenda nos dois idiomas. As imagens ficam em `website/static/img/`.

- Telas paradas em PNG. Animações em WebM, num `<video autoplay loop muted playsinline>` com o PNG como `poster`. GIF foi considerado e deixado de lado: uma animação de 20 s da TUI passa fácil de 5 MB em GIF e fica abaixo de 1 MB em WebM.
- Teto de 2 MB por arquivo, conferido pelo teste.
- Telas previstas, a confirmar na implementação:
  - as abas com a barra;
  - o painel de rede com um bloqueio;
  - a página de configurações com os temas;
  - o diagnóstico;
  - o grafo do cofre com um segredo de demonstração;
  - a árvore de processos.

### 6. Roteiros do VHS isolados

`website/tapes/*.tape` são roteiros do VHS, um por tela, rodados por `scripts/site-screens.sh`. O script:

1. compila os binários (com `CARGO_BUILD_JOBS=16`) e põe a pasta deles no `PATH` da demonstração;
2. cria uma pasta temporária com `XDG_RUNTIME_DIR` (modo 0700), `XDG_DATA_HOME`, `XDG_CONFIG_HOME` e `XDG_STATE_HOME` próprios, um HOME de demonstração e uma pasta de projeto de exemplo;
3. exporta `SANDMUX_DAEMON_START=direct`, para não instalar nem usar a unidade systemd do usuário, e `SANDMUX_HELPER_SOCKET` para um caminho dentro da pasta temporária que não existe. A demonstração nunca fala com o auxiliar root e nunca carrega BPF;
4. exporta `SANDMUX_LANG` por roteiro (`pt` e `en`) e um tema fixo;
5. roda cada roteiro com o VHS, que grava em `website/static/img/`;
6. no fim, e também num `trap`, para o daemon da demonstração com o mesmo ambiente, confere pelo caminho do executável que não sobrou processo da demonstração e apaga a pasta temporária.

Os roteiros usam o preset `shell` com `--no-import`. O cofre da demonstração é criado na hora, com uma senha e um valor de demonstração escritos no próprio roteiro (`demo-nao-e-uma-chave`). O VHS sobe um navegador headless; o script roda sob o vigia de memória usado nos testes pesados.

Por idioma: as telas da página em inglês são gravadas com `SANDMUX_LANG=en`. O `screens.toml` aponta o arquivo de cada idioma.

### 7. Testes no `docs.rs`

Rodam no `check.sh`, sem o Hugo:

- todo `docs/*.md` está no `website/data/nav.toml`, e todo item do menu tem o arquivo;
- todo `docs/*.md` começa com uma linha `# `;
- todo `website/content/**/*.md` em português tem o par `.en.md`;
- todo arquivo de `website/static/img/` tem até 2 MB, e todo arquivo citado no `screens.toml` existe;
- nenhum layout nem CSS de `website/` carrega recurso de outro endereço: `src="http`, `<link ... href="http` ou `url(http`. Links `<a href>` para fora são permitidos.

A teste de links que já existe continua valendo para os docs no GitHub; os links no site ficam com o render hook, no build.

### 8. Publicação pelo GitHub Actions

`.github/workflows/pages.yml`:

- Gatilhos:
  - `push` em `main` e `pull_request`, os dois com filtro de caminho (`website/**`, `docs/**`, `README.md`, `README.en.md` e o próprio workflow);
  - `workflow_dispatch`.
- Job `build`:
  - baixa o Hugo de uma versão exata da release oficial e confere o SHA-256 com o arquivo de somas da própria release, como o `install.sh` faz com o sandmux;
  - roda `hugo --source website --minify --gc --panicOnWarning`;
  - sobe `website/public` com `actions/upload-pages-artifact`.
- Job `deploy`: só fora de pull request; `needs: build`, no ambiente `github-pages`, com `actions/deploy-pages`.
- `permissions` mínimas: `contents: read`, e `pages: write` e `id-token: write` só no deploy. `concurrency: pages`, sem cancelar o deploy em andamento.

Só ações oficiais (`actions/*`), sem ação de terceiros para o Hugo. Um build que falha não chega ao deploy, e o Pages continua com a versão anterior.

Alternativa: um branch `gh-pages` empurrado pelo workflow. Ele guarda um histórico de HTML gerado no repositório e precisa de `contents: write`.

### 9. Os docs novos

`docs/architecture.md` e `docs/security.md`, com os pares, escritos a partir das specs vigentes (`openspec/specs/`), da seção 3 do roadmap e dos docs existentes. Cada afirmação é conferida contra a spec atual, e não contra o roadmap, que tem trechos antigos: a seção 3.3 ainda chama de roadmap o nível do BPF, que já existe com o auxiliar root. Os diagramas ficam em ASCII, em blocos de código, para aparecerem iguais no GitHub e no site.

A segurança traz:

- o modelo de ameaça: um agente manipulado por prompt injection;
- por camada, o que ela impede;
- o que fica fora do escopo: exploit de kernel, e o que o usuário libera de propósito;
- os limites conhecidos, tirados da seção 6 do roadmap: CDN no modo aberta, IP direto, o que o agente pode fazer com o que vê na pasta de trabalho.

## Risks / Trade-offs

- [Mount com `source = "../docs"`, fora da pasta do projeto Hugo] → o Hugo aceita caminho relativo no módulo principal; se a versão fixada recusar, o workflow e o servidor local passam a apontar `--source` para a raiz com `contentDir` e `themesDir` ajustados, ou montam `docs/` por caminho absoluto. A primeira tarefa é provar o mount.
- [Âncora conferida em outra página dentro do render hook pode causar recursão de render no Hugo] → se acontecer, a conferência da âncora vai para o teste do `docs.rs`, que calcula o slug do GitHub a partir dos títulos, e o hook fica só com a existência da página.
- [Título tirado do `.RawContent`] → depende de todo doc começar com `# `, o que o teste garante.
- [Screenshots desatualizados depois de mudar a TUI] → nada falha quando a TUI muda. O script é um comando só, e o AGENTS.md ganha a regra de regenerar as telas quando um change muda o que elas mostram.
- [Rodar o VHS pesa na máquina (navegador headless mais o sandmux)] → um roteiro de cada vez, sob o vigia de memória.
- [Hugo e VHS como ferramentas novas] → só para quem mexe no site. O Hugo é baixado com a soma conferida no CI. Na máquina de desenvolvimento, a instalação dele é feita com o OK do usuário.
- [O Pages precisa ser ligado nas configurações do repositório, com a fonte "GitHub Actions"] → passo do usuário. Até lá, o deploy falha, e o build do pull request funciona.
- [Um doc com HTML solto no futuro] → o Goldmark do Hugo escapa HTML por padrão (`unsafe = false`). Fica assim; hoje nenhum doc usa HTML.

## Migration Plan

1. Merge do change com o workflow. O build roda no pull request.
2. O usuário liga o Pages com a fonte "GitHub Actions".
3. O primeiro push em `main` publica o site, ou o `workflow_dispatch`.
4. Para desfazer: desligar o Pages ou remover o workflow. Nada no sandmux depende do site.

## Open Questions

- A versão exata do Hugo é a estável mais nova no dia da implementação, desde que tenha o layout de templates novo (`layouts/_markup/`, 0.146 ou mais nova).
- A lista final de telas e a ordem da galeria podem mudar depois de ver as primeiras gravações. O formato e o teto não mudam.
