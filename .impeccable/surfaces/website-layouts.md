---
version: 1
slug: "website-layouts"
primary_target: "website/layouts"
related_targets: ["website/content"]
---

# Site do sandmux

Escopo: a página inicial (Persuade) e as páginas da documentação (Read), em `website/layouts/`. Público e produto em PRODUCT.md.

Ação principal: instalar (`curl ... install.sh | sh`) ou abrir a documentação. Prova: a TUI real, gravada pelo VHS, e a lista do que o agente enxerga.

## Direction contract

THESIS: o padrão das páginas de ferramenta para desenvolvedores, com o acabamento do Ghostty e do Zed: o produto é a estrela, a cópia é curta e verificável. Recusa a fileira de três cards iguais, o gradiente no título e o neon no escuro.

OWN-WORLD: fundo grafite neutro no escuro (#141414, da mesma família do #282828 das telas da TUI) e papel morno no claro, texto em Clear Sans, código e terminal em Fira Code, um único destaque em cor de areia (#e8b04b no escuro; #8f6210 no claro, mais escuro que o #9a6a12 do logo para o texto de link passar de 4,5:1 sobre o papel) para links, foco, seleção e o botão principal. Janelas de terminal com moldura fina e cantos de 12px. O logo é o prompt dentro da caixa.

STORY: quem chega entende em uma tela que cada agente roda numa aba confinada, vê a TUI funcionando, copia o comando de instalação; quem avalia segurança acha o modelo de ameaça e os limites em um clique.

FIRST VIEWPORT: cabeçalho com o logo, Documentação, Arquitetura, Segurança, GitHub e o idioma. O título em duas linhas à esquerda e o resumo com a situação à direita, alinhados pela base; abaixo, numa linha só, o comando de instalação inteiro numa caixa de terminal e o botão "Instalar e começar" (leva ao guia de instalação, a ação que o visitante veio fazer; a documentação fica no cabeçalho). A janela da TUI, na largura toda, começa na primeira dobra, com a animação em loop que já abre mostrando o .env recusado. A divisão em duas colunas com a janela ao lado foi descartada no build porque o texto do terminal ficava ilegível.

FORM: canon do segmento (escolha do usuário), referências Ghostty e Zed. seed:7cfd2196e8c9258c

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
