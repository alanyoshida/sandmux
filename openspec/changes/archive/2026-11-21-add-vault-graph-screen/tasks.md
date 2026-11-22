## 1. Modelo, posições e janela

- [x] 1.1 Criar `tui/vaultgraph.rs` com o modelo: de uma lista de `VaultEntry`, os domínios em ordem alfabética com os seus segredos, o grupo sem domínio e as entradas de tipo `db` fora dos dois. Verificar com testes de unidade: segredo em três domínios aparece nos três, `github.com` e `*.github.com` separados, grupo sem domínio só quando há segredo sem domínio, entrada `db` em nenhum dos dois
- [x] 1.2 Posições: o domínio na origem e os segredos em anéis de 8, 12, 16 e assim por diante (4 a mais por anel), em ângulos iguais e ordem alfabética. Verificar com testes de unidade para 1, 8, 9, 20 e 21 segredos (um, um, dois, dois e três anéis; nenhum par de nós na mesma posição)
- [x] 1.3 Janela: os sete passos de zoom com os limites 0.5x e 2.0x, o movimento que para na caixa dos nós, a conversão de ponto do mundo para célula e o nível de detalhe por zoom (0.7x e 1.3x). Verificar com testes de unidade dos limites, da conversão e dos três níveis

## 2. Configuração

- [x] 2.1 Chave `graph-edges` de `[tui]` em `sandmux-policy`: leitura, validação (`"braille"` ou `"blocos"`, padrão `"braille"`) e editor de configuração. Verificar com testes de `config`: os dois valores aceitos, valor inválido recusado com a linha e os valores aceitos
- [x] 2.2 Item de escolha "Arestas do grafo" na seção Aparência da página de configurações, nos dois idiomas. Verificar com um teste da TUI que troca a escolha e confere `graph-edges = "blocos"` no `config.toml`
- [x] 2.3 Documentar a chave em `docs/config.md` e `docs/en/config.md`. Verificar com os testes de `docs.rs`

## 3. Tela só de leitura

- [x] 3.1 Confirmar que o `Canvas` do ratatui compila com os recursos atuais do `Cargo.toml` e, se não, acrescentar o recurso que falta. Verificar com `cargo check -p sandmux` (com `CARGO_BUILD_JOBS=16`)
- [x] 3.2 Estado `graph` em `VaultView` e a tecla `g` na seção Cofre: abre a tela com o cofre aberto ou trancado, só avisa sem cofre; Esc fecha. Incluir `g` no rodapé da seção. Verificar com um teste da TUI que abre, vê a lista de domínios com as contagens e volta com Esc
- [x] 3.3 Desenho da tela: lista de domínios com o grupo "Sem domínio", grafo com as arestas no `Canvas` e os nós por cima, detalhes do segredo em foco, rodapé com as teclas e o zoom; marca por tipo, cor por estado e `+N` nos segredos de vários donos. Verificar com um teste da TUI sobre um cofre de exemplo: centro e segredos do domínio escolhido, `+2` no segredo de três domínios, detalhes sem o valor, e o valor real ausente da tela inteira
- [x] 3.4 Foco e filtro: `↑` `↓`, `Tab`, `→`, `←`, `o` e `/`. Verificar com um teste da TUI: foco no primeiro segredo, `o` troca de dono mantendo o segredo, `/npm` deixa só o dono de `npm-token`
- [x] 3.5 Cofre trancado: a tela avisa e `u` abre o formulário de desbloqueio. Verificar com um teste da TUI que tranca, abre a tela, desbloqueia por ela e vê o grafo
- [x] 3.6 Grupo "Bancos": pedir `DbList` ao abrir e ao reler, item no fim da lista só quando há conexão, conexões com a marca `■`, detalhes com tipo, servidor, porta, banco e usuário, e o aviso de `sandmux db` nas teclas de edição. Verificar com um teste da TUI que cria uma conexão por `sandmux db add`, vê o grupo e os detalhes sem a senha, aperta `x` e confere que a conexão continua em `sandmux db list`
- [x] 3.7 Textos nos dois idiomas com `tr!` e `trf!`. Verificar com os testes de `i18n.rs`

## 4. Zoom, movimento e largura

- [x] 4.1 Teclas `+`, `=`, `-`, `h`, `j`, `k`, `l` e `0`, com o zoom no rodapé e os três níveis de detalhe no desenho. Verificar com um teste da TUI: `+` até 1.6x mostra o cartão com `token · gh git`, `-` até 0.5x deixa só as marcas, `+` em 2.0x não passa de 2.0x, `0` volta a 1.0x
- [x] 4.2 Mouse na tela do grafo: roda para o zoom, arrastar para mover, clique na lista e num nó, com as áreas de clique guardadas no desenho. Verificar com um teste da TUI que manda as sequências SGR: o centro anda 10 colunas ao arrastar, a roda muda o zoom do rodapé, o clique troca o domínio
- [x] 4.3 Largura: três partes a partir de 100 colunas; lista e grafo de 70 a 99, com os detalhes em caixa por Enter; só a lista abaixo de 70, com o aviso. Verificar com testes da TUI em 120, 80 e 60 colunas
- [x] 4.4 Arestas em meio-bloco com `graph-edges = "blocos"`. Verificar com um teste da TUI que não acha nenhum caractere de U+2800 a U+28FF na tela, e que acha com o padrão

## 5. Edição pela tela

- [x] 5.1 `a` e `s` com o domínio escolhido já no campo de domínios (`VaultForm::prefill`), formulário desenhado sobre o grafo. Verificar com um teste da TUI: segredo novo aparece no grafo do domínio, o valor digitado nunca aparece, `sandmux vault list` mostra o segredo com o domínio
- [x] 5.2 `d`, `v` e `x` no segredo em foco, com a confirmação de apagar; depois de cada mudança, reler o cofre e manter o domínio e o segredo pelo nome, ou o vizinho quando o domínio some. Verificar com um teste da TUI: tirar um domínio some o segredo daquele grafo e mantém no outro; apagar o único segredo de um domínio tira o domínio da lista
- [x] 5.3 `r` relê o cofre e redesenha. Verificar com um teste da TUI que acrescenta um segredo por `sandmux vault add` com a tela aberta e o vê depois de `r`

## 6. Documentação e conferência

- [x] 6.1 `docs/vault.md` (seção "Pela TUI": a tecla `g`, a tela, os grupos "Sem domínio" e "Bancos", as teclas e o que ela nunca mostra) e `docs/tui.md` (seção "Configurações", linha do Cofre, e "Mouse"), com os pares em inglês. Verificar com os testes de `docs.rs`
- [x] 6.2 Rodar `./scripts/check.sh` inteiro sob o vigia, com `CARGO_BUILD_JOBS=16`, e conferir que termina verde
- [x] 6.3 Conferência à mão do usuário no build local: as arestas em Braille no terminal dele, o zoom e o arrastar com o mouse, e a troca para `graph-edges = "blocos"`

## Workflow follow-up

- Commit e push num branch novo, PR, merge e release nova (ordem do usuário, 2026-11-17).
- Arquivar o change e sincronizar as specs.
- Atualizar o `openspec/roadmap.md` a cada passo.
