# project-site Specification

## Purpose
Define o site público do sandmux no GitHub Pages: a página inicial com screenshots, a documentação navegável lida dos arquivos de `docs/` como fonte única, os dois idiomas, os screenshots gerados fora do CI e a publicação.

## Requirements

### Requirement: Endereço do site
O site MUST ser publicado em `https://alanyoshida.github.io/sandmux/`, com a página inicial na raiz e a documentação em `docs/`. Todo link interno MUST levar em conta o prefixo `/sandmux/`.

#### Scenario: Página inicial
- **WHEN** alguém abre `https://alanyoshida.github.io/sandmux/`
- **THEN** a página inicial em português aparece, e o link para a documentação leva a uma página que existe no site

### Requirement: Página inicial
A página inicial MUST apresentar o projeto para quem não o conhece: o problema que ele resolve, o que o agente enxerga dentro do sandbox, screenshots da TUI, o comando de instalação em uma linha e links para a documentação, a arquitetura, a segurança, as releases e o repositório. MUST dizer que o projeto está antes da v1.

#### Scenario: Visitante novo
- **WHEN** alguém abre a página inicial
- **THEN** vê ao menos um screenshot da TUI, o `curl ... install.sh | sh` e os links para arquitetura e segurança

### Requirement: Dois idiomas
O site MUST existir em português, sem prefixo na URL, e em inglês, sob `/en/`. Toda página MUST ter o par no outro idioma e um link que leva ao par, não à página inicial do outro idioma.

#### Scenario: Troca de idioma numa página da documentação
- **WHEN** alguém está em `/sandmux/docs/network/` e escolhe English
- **THEN** abre `/sandmux/en/docs/network/`

#### Scenario: Página inicial em inglês
- **WHEN** alguém abre `/sandmux/en/`
- **THEN** vê a página inicial em inglês, com os mesmos screenshots

### Requirement: Documentação como fonte única
Cada arquivo `docs/<nome>.md` MUST virar a página `docs/<nome>/` em português, e `docs/en/<nome>.md` a mesma página em inglês, lidos dos arquivos do repositório no momento do build. O repositório MUST NOT ter uma segunda cópia do texto dos docs para o site. O título da página MUST ser o primeiro título do arquivo.

#### Scenario: Doc alterado
- **WHEN** um change altera `docs/vault.md` e `docs/en/vault.md` e o site é publicado de novo
- **THEN** as duas páginas do cofre mostram o texto novo, sem nenhum outro arquivo alterado

#### Scenario: Doc novo
- **WHEN** um change cria `docs/foo.md` e `docs/en/foo.md` e os põe no menu
- **THEN** o site ganha `docs/foo/` e `en/docs/foo/` sem criar nenhum arquivo de conteúdo em `website/`

### Requirement: Links entre os docs
Um link relativo para outro doc (`network.md`, `vault.md#grafo-do-cofre`, `../install.md`) MUST levar à página correspondente do site, mantendo a âncora; um link do doc em inglês para `../<nome>.md` MUST levar à página em português. Os mesmos arquivos MUST continuar com os links válidos no GitHub. Um link para um doc que não existe MUST falhar o build do site.

#### Scenario: Link com âncora
- **WHEN** a página `docs/tui/` tem o link `vault.md#grafo-do-cofre`
- **THEN** o link leva a `/sandmux/docs/vault/#grafo-do-cofre`, e a âncora existe na página

#### Scenario: Link quebrado
- **WHEN** um doc aponta para `nao-existe.md`
- **THEN** o build do site falha com o nome do arquivo e do link

#### Scenario: Link para fora dos docs
- **WHEN** um doc aponta para um arquivo do repositório que não é doc, como `../LICENSE`
- **THEN** o link leva ao arquivo no GitHub

### Requirement: Menu da documentação
Toda página da documentação MUST ter um menu lateral com todos os docs, agrupados por seção, nos dois idiomas. Todo arquivo de `docs/` MUST estar no menu; um doc fora do menu MUST falhar os testes do projeto.

#### Scenario: Doc esquecido no menu
- **WHEN** um change cria `docs/foo.md` e não o põe no menu
- **THEN** o teste da documentação falha dizendo que `foo.md` não está no menu do site

### Requirement: Arquitetura e segurança
A documentação MUST ter `docs/architecture.md` e `docs/security.md`, com os pares em inglês, escritos para quem não conhece o projeto. A arquitetura MUST cobrir os binários e as fronteiras de confiança, as camadas do sandbox, a rede pelo gateway e o cofre. A segurança MUST cobrir o modelo de ameaça, o que cada camada impede, o que fica fora do escopo e os limites conhecidos.

#### Scenario: Fora do escopo declarado
- **WHEN** alguém lê `docs/security.md`
- **THEN** encontra que um exploit de kernel está fora do escopo e os limites conhecidos do modo aberta

#### Scenario: Também no GitHub
- **WHEN** alguém abre `docs/architecture.md` no GitHub
- **THEN** o texto e os diagramas aparecem, e os links para os outros docs funcionam

### Requirement: Screenshots gerados fora do CI
Os screenshots e gifs do site MUST ser arquivos commitados, produzidos por roteiros versionados que rodam o sandmux real na máquina de desenvolvimento. O build e a publicação do site MUST NOT rodar o sandmux.

#### Scenario: TUI mudou
- **WHEN** quem desenvolve roda o comando que regenera as imagens depois de mudar a TUI
- **THEN** as imagens em `website/` são substituídas, e o próximo build publica as novas

#### Scenario: Build no CI
- **WHEN** o workflow do site roda
- **THEN** nenhum binário do sandmux é compilado nem executado

### Requirement: Screenshots sem dados do usuário
Os roteiros MUST rodar com pastas de configuração, dados e estado próprias e temporárias, numa pasta de demonstração temporária, sem importar a configuração dos harnesses e sem nenhum segredo real. As imagens MUST NOT mostrar caminho do HOME real, sessão, domínio ou segredo do usuário.

#### Scenario: Máquina com sessões e cofre
- **WHEN** quem desenvolve regenera as imagens com sessões vivas e o cofre aberto no próprio usuário
- **THEN** as imagens mostram só as sessões e os dados da demonstração, e as sessões do usuário continuam como estavam

### Requirement: Publicação do site
Um push em `main` que altera `website/`, `docs/`, os READMEs ou o workflow do site MUST montar e publicar o site no GitHub Pages; o workflow MUST poder ser rodado à mão. Num pull request, o site MUST ser montado sem publicar. Um build que falha MUST NOT publicar, e a versão anterior continua no ar.

#### Scenario: Pull request com link quebrado
- **WHEN** um pull request introduz um link para um doc que não existe
- **THEN** o check do site falha no pull request, e nada é publicado

#### Scenario: Merge de um doc
- **WHEN** um pull request que altera `docs/network.md` entra em `main`
- **THEN** o site publicado mostra o texto novo depois do workflow

### Requirement: Site sem terceiros
As páginas MUST NOT carregar scripts, fontes, estilos, imagens ou analytics de outro endereço; tudo vem do próprio site. O site MUST funcionar sem JavaScript, salvo o que for só conveniência.

#### Scenario: Página aberta com a rede do navegador observada
- **WHEN** alguém abre qualquer página do site
- **THEN** todos os pedidos vão para `alanyoshida.github.io`

### Requirement: Link para o site no README
O `README.md` e o `README.en.md` MUST ter um link para o site, cada um para a versão no seu idioma.

#### Scenario: README em inglês
- **WHEN** alguém abre o `README.en.md`
- **THEN** encontra o link para `https://alanyoshida.github.io/sandmux/en/`
