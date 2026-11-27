# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Site estático em Hugo, em `website/`, publicado no GitHub Pages por `.github/workflows/pages.yml` (decisão do usuário, 2026-11-27). Layouts próprios, um CSS único, sem JavaScript, sem Node e sem Sass. Nenhum recurso de outro endereço: sem CDN, web font nem analytics. Detalhes no change `add-project-site`.

## Users

- Desenvolvedores que já rodam harnesses de agente de código (opencode, Claude Code, hermes, oh-my-pi, Antigravity) no Linux e não querem que o agente leia segredos do HOME ou mande dados para fora. Chegam pelo repositório ou por uma indicação e decidem se instalam.
- Quem avalia ferramentas de segurança (time de segurança, revisor técnico) e precisa entender o modelo de ameaça, as camadas e os limites antes de aprovar o uso (confirmado pelo usuário, 2026-11-27).

## Product Purpose

O sandmux é um multiplexador de terminal no estilo do tmux, em Rust com ratatui, que sobe cada agente numa aba confinada: o agente enxerga a pasta de trabalho e mais nada do HOME, a rede passa por um gateway que registra e filtra, e nenhuma credencial real entra no sandbox. O site existe para mostrar a TUI funcionando, explicar a arquitetura e a segurança a quem não conhece o projeto e publicar a documentação inteira.

## Positioning

- O foco é o sandbox, não a interface. Toda aba tem sandbox; não existe shell livre.
- Nenhum segredo real dentro do sandbox: o agente recebe valores falsos, e o gateway troca pelo real no tráfego, só quando o destino é o dono da chave.
- Primitivas do kernel Linux (Landlock, seccomp, namespaces), sem VM nem contêiner, e sem root para o uso normal.
- Seguro por padrão, com o mínimo de atrito: uma baseline embutida funciona sem configurar nada; o usuário só gerencia exceções.

## Operating Context

O usuário vive no terminal: entra na pasta do projeto, roda `sandmux run <harness>`, troca de aba com Ctrl+b como no tmux, e acompanha a rede e os arquivos no painel lateral. Instala com `curl ... install.sh | sh`, pacotes deb e rpm, AUR, Homebrew ou `cargo install`. O revisor de segurança lê `docs/security.md`, `docs/architecture.md` e as specs.

## Capabilities and Constraints

- Apenas Linux, `x86_64`, kernel 6.12 ou mais novo (ou o 6.7 a 6.11 com o auxiliar root).
- Em desenvolvimento, antes da v1. O site precisa dizer isso.
- Português e inglês em tudo; o português é a fonte de verdade.
- Ameaça alvo: um agente manipulado por prompt injection tentando ler segredos e exfiltrar dados. Exploit de kernel fora do escopo.
- Licença MIT.

## Brand Commitments

- Nome: `sandmux`, sempre em minúsculas.
- Tom: o do README. Direto, técnico, frases curtas, sem promessa exagerada; diz o que funciona e o que falta.
- Logo: ainda não existe. O usuário pediu opções para escolher (2026-11-27).
- Visual do site: o padrão das páginas de ferramenta para desenvolvedores, executado com capricho, sem ironia nem enfeite (escolha do usuário, 2026-11-27). A régua de acabamento são os sites do Ghostty e do Zed.

## Evidence on Hand

- Screenshots e gravações da TUI real, gerados por `scripts/site-screens.sh` com o VHS.
- README, `docs/` e as specs em `openspec/specs/`.
- Medidas registradas no roadmap (por exemplo, o custo da interceptação de arquivos por seccomp user notify, de 1,0x a 1,3x no uso normal).
- Não existem depoimentos, clientes, número de usuários nem benchmarks comparativos. O site não inventa nenhum.

## Product Principles

1. O que o site afirma sobre segurança é verificável na spec ou no código; limites conhecidos aparecem, não se escondem.
2. Mostrar antes de explicar: a TUI real vale mais que um adjetivo.
3. Respeito ao visitante como o sandmux respeita o usuário: nada de rastreio, nada carregado de terceiros.
4. Um caminho curto do interesse à instalação.

## Accessibility & Inclusion

Contraste WCAG AA nos dois temas, leitura sem JavaScript, blocos de código e diagramas ASCII legíveis no celular, texto alternativo em toda imagem.
