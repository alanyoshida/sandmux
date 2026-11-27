## Why

O sandmux só se apresenta pelo README e pelos arquivos de `docs/`, lidos no GitHub. Não há um lugar que mostre a TUI funcionando, nem um texto que explique a arquitetura e o modelo de segurança para quem ainda não conhece o projeto: essas explicações existem só no roadmap e nas specs, escritas para o planejamento. O usuário pediu em 2026-11-27 um site no GitHub Pages com screenshots e uma documentação de arquitetura e segurança.

## What Changes

- **Site do projeto** em `https://alanyoshida.github.io/sandmux/`, gerado com Hugo (decisão do usuário, 2026-11-27):
  - uma página inicial com a apresentação do projeto, screenshots e gifs da TUI, a instalação em uma linha e os links para a documentação e as releases;
  - a documentação inteira navegável, com um menu lateral por seções;
  - português como idioma padrão, sem prefixo na URL, e inglês em `/en/`, com troca de idioma em toda página.
- **Fonte única** (decisão do usuário): o site lê `docs/*.md` e `docs/en/*.md` como estão, sem cópia. Os links entre os docs continuam funcionando no GitHub e passam a funcionar também no site.
- **Dois docs novos**, nos dois idiomas, escritos para quem não conhece o projeto: `docs/architecture.md` (binários, fronteiras de confiança, camadas do sandbox, rede pelo gateway, cofre) e `docs/security.md` (modelo de ameaça, o que cada camada impede, o que fica fora do escopo e os limites conhecidos). Ficam em `docs/` para valerem também no GitHub e cair na regra do par em inglês.
- **Screenshots gerados localmente** (decisão do usuário): roteiros do VHS versionados em `website/tapes/` rodam o sandmux real na máquina de desenvolvimento, com pastas de dados isoladas e sem nenhum segredo, e as imagens resultantes são commitadas. O CI nunca roda o sandmux.
- **Publicação**: um workflow novo, `.github/workflows/pages.yml`, monta o site com o Hugo e publica pelo `actions/deploy-pages` a cada push em `main` que toca o site ou os docs. Num pull request, só monta, para um link quebrado falhar antes do merge.
- **Link para o site** no README, nos dois idiomas.

## Capabilities

### New Capabilities

- `project-site`: o site público do projeto: o que ele mostra, os dois idiomas, a leitura dos docs como fonte única, os screenshots gerados fora do CI e a publicação.

### Modified Capabilities

Nenhuma. A regra do par em inglês de `interface-language` já cobre os dois docs novos.

## Impact

- Pasta nova `website/` (fora do workspace Cargo): configuração do Hugo, layouts próprios em HTML e CSS, sem tema de terceiros, sem Node e sem Sass; a página inicial nos dois idiomas; o menu da documentação; os roteiros do VHS e as imagens.
- `docs/architecture.md`, `docs/security.md` e os pares em `docs/en/`; um link para o site no `README.md` e no `README.en.md`.
- `.github/workflows/pages.yml` novo. O `release.yml` não muda.
- `crates/sandmux/tests/docs.rs`: todo doc aparece no menu do site, e a página inicial tem o par em inglês.
- Ferramentas, só para quem mexe no site: o Hugo (versão fixada no workflow) e o VHS. Nenhuma dependência nova nos binários.
- Configuração do repositório no GitHub: o Pages com a fonte "GitHub Actions", ligado pelo usuário.
