# Tasks

Compilações e testes com `CARGO_BUILD_JOBS=16`, sob o vigia de memória. Baixar e instalar o Hugo na máquina de desenvolvimento depende do OK do usuário. Ligar o Pages no repositório é um passo do usuário.

## 1. Esqueleto do Hugo

- [x] 1.1 Instalar o Hugo na máquina de desenvolvimento, com o OK do usuário, na versão estável mais nova com `layouts/_markup/` (0.146 ou mais nova), e anotar a versão. Conferir com `hugo version`
- [x] 1.2 Criar `website/hugo.toml` com o `baseURL`, os dois idiomas (pt na raiz, en em `/en/`), `autoHeadingIDType = "github"`, `disableKinds` e os mounts de `content`, `../docs` (pt, sem `en/**`), `../docs/en` (en), `layouts`, `static`, `data`, `i18n` e `assets`. Pôr `website/public/`, `website/resources/_gen/` e `website/.hugo_build.lock` no `.gitignore`. Conferir com `hugo --source website` que saem `public/docs/network/index.html` e `public/en/docs/network/index.html`, e que a troca de idioma de uma acha a outra (`.Translations`). Se o mount de `../docs` for recusado, aplicar a alternativa do design antes de seguir

## 2. Layouts, menu e links

- [x] 2.1 Partial `doc-title` (primeiro `# ` do `.RawContent`), `website/data/nav.toml` com as seções do design, `website/i18n/pt.toml` e `en.toml`, e as páginas `website/content/docs/_index.md` e `_index.en.md`. Conferir no `public/` que o `<title>` e o menu de `docs/vault/` e de `en/docs/vault/` mostram o título do doc no idioma certo
- [x] 2.2 Render hook de link com a tabela do design e o `errorf` para doc ou âncora inexistente; render hook de imagem para `website/static/`. Conferir no HTML gerado:
  - `vault.md#grafo-do-cofre` em `docs/tui/` vira `/sandmux/docs/vault/#grafo-do-cofre`;
  - o `../install.md` de `en/docs/install/` leva a `/sandmux/docs/install/`;
  - um link para fora de `docs/` leva ao GitHub.

  Conferir também que um link temporário para `nao-existe.md` e uma âncora inexistente falham o build com o nome do arquivo. Se a conferência da âncora causar recursão, aplicar a alternativa do design (a âncora conferida no `docs.rs`)
- [x] 2.3 Layouts de base, de doc (menu lateral, índice da página, troca de idioma, `<details>` no celular) e o CSS único com `Minify` e `Fingerprint`, claro e escuro, fontes do sistema, CSS do Chroma gerado no site. Conferir com `hugo server --source website` no navegador embutido, em largura de celular e de desktop, nos dois temas, com `docs/network/` (tabelas, blocos de código, diagramas ASCII), e que a página não faz pedido a outro endereço (lista de requisições do navegador)
- [x] 2.4 Testes no `crates/sandmux/tests/docs.rs`:
  - todo `docs/*.md` no `nav.toml`, e todo item do menu com arquivo;
  - todo `docs/*.md` começa com `# `;
  - todo `website/content/**/*.md` em português com o par `.en.md`;
  - nenhum recurso de outro endereço nos layouts e no CSS.

  Conferir que passam, e que cada um falha com um doc temporário fora do menu, uma página sem par e um `<script src="https://...">` temporário

## 3. Arquitetura e segurança

- [x] 3.1 Escrever `docs/architecture.md` e `docs/en/architecture.md`:
  - os binários e as fronteiras de confiança;
  - as camadas do sandbox;
  - os níveis de proteção, com o auxiliar root e o BPF LSM como existem hoje;
  - a rede pelo gateway;
  - o cofre e a troca de falsos;
  - os diagramas em ASCII.

  Conferir cada afirmação contra as specs vigentes (`sandbox-isolation`, `sandbox-filesystem`, `network-gateway`, `secret-vault`, `root-helper`, `session-daemon`). Conferir que o teste do par e dos links passa e que a página aparece no menu
- [x] 3.2 Escrever `docs/security.md` e `docs/en/security.md`:
  - o modelo de ameaça;
  - o que cada camada impede;
  - o que fica fora do escopo (exploit de kernel, o que o usuário libera de propósito);
  - os limites conhecidos da seção 6 do roadmap (CDN e SNI no modo aberta, IP direto, o que o agente faz com o que vê na pasta de trabalho).

  Conferir contra as specs e o roadmap, e conferir os testes do `docs.rs`
- [x] 3.3 Ligar os dois docs novos a partir dos docs existentes que tratam do assunto (`helper.md`, `network.md`, `vault.md`), nos dois idiomas, e conferir que o teste de links passa

## 4. Screenshots

- [x] 4.1 `scripts/site-screens.sh` com o isolamento do design:
  - XDG temporários (runtime 0700);
  - `SANDMUX_DAEMON_START=direct`;
  - `SANDMUX_HELPER_SOCKET` para um caminho inexistente;
  - HOME e projeto de demonstração;
  - `SANDMUX_LANG` por roteiro;
  - `trap` que para o daemon da demonstração e apaga a pasta.

  Conferir rodando com um roteiro mínimo:
  - `sandmux ls` do usuário, sem o ambiente da demonstração, continua igual antes e depois;
  - a unidade systemd de usuário não foi tocada (`systemctl --user cat sandmuxd` igual);
  - o auxiliar root não recebeu pedido (log do serviço);
  - pelo caminho do executável, não sobra processo da demonstração.

  Relatar o que sobrar
- [x] 4.2 Roteiros do VHS em `website/tapes/` para as telas do design, em `pt` e `en`, com o preset `shell`, `--no-import`, o cofre e o segredo de demonstração (`demo-nao-e-uma-chave`). Gravar com o script sob o vigia, PNG para as telas paradas e WebM com o PNG de `poster` para as animações. Abrir cada imagem e conferir que não aparece caminho do HOME real, sessão, domínio ou segredo do usuário
- [x] 4.3 `website/data/screens.toml` com arquivo, tipo e legenda nos dois idiomas, e o teste no `docs.rs`: arquivos do `screens.toml` existem e todo arquivo de `website/static/img/` tem até 2 MB. Conferir que o teste falha com um arquivo temporário de 3 MB
- [x] 4.4 Regra no `AGENTS.md`: um change que muda o que uma tela do site mostra roda `scripts/site-screens.sh` e commita as imagens. Conferir o texto

## 5. Página inicial

- [x] 5.1 `website/content/_index.md` e `_index.en.md` com a apresentação:
  - o problema;
  - o que o agente enxerga, com a tabela do README;
  - a galeria;
  - o `curl ... install.sh | sh`;
  - o resumo das camadas;
  - o aviso de antes da v1;
  - os links para a documentação, a arquitetura, a segurança, as releases e o repositório.

  Layout da página inicial com a galeria. Conferir no navegador embutido, nos dois idiomas, em celular e desktop, que os vídeos tocam sem som e em loop, e que todo link leva a uma página que existe
- [x] 5.2 Link para o site no `README.md` (`/sandmux/`) e no `README.en.md` (`/sandmux/en/`). Conferir o teste do par

## 6. Publicação

- [x] 6.1 `.github/workflows/pages.yml` como no design:
  - gatilhos com filtro de caminho;
  - o Hugo da versão anotada, baixado com o SHA-256 conferido;
  - `hugo --source website --minify --gc --panicOnWarning`;
  - `upload-pages-artifact`, e o `deploy-pages` só fora de pull request;
  - `permissions` mínimas e `concurrency`.

  Conferir com `actionlint`, se disponível, ou com a leitura do YAML, e com o mesmo comando de build rodado localmente
- [x] 6.2 Rodar `./scripts/check.sh` sob o vigia e conferir que passa, com os testes novos do `docs.rs`; conferir que não sobrou processo de teste pelo caminho do executável

## 7. Integração

- [x] 7.1 Montar o site inteiro do zero (`rm -rf website/public && hugo --source website --panicOnWarning`) e percorrer no navegador embutido:
  - a página inicial;
  - três docs de seções diferentes;
  - a troca de idioma em cada um;
  - um link com âncora;
  - um link para o GitHub.

  Conferir que nenhuma página faz pedido a outro endereço. Atualizar o `openspec/roadmap.md` com a situação do change

## Workflow follow-up

- O usuário liga o GitHub Pages no repositório com a fonte "GitHub Actions" (ou autoriza o `gh api` que faz isso).
- Commit e push num branch novo, pull request, conferir o check do site no pull request, merge.
- Conferir o deploy no `main` e abrir `https://alanyoshida.github.io/sandmux/` e `/en/`.
- Arquivar o change e sincronizar a spec `project-site`, com o roadmap atualizado.
