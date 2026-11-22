## ADDED Requirements

### Requirement: Tela do grafo do cofre
Na seção "Cofre" da página de configurações, a tecla `g` MUST abrir uma tela cheia com a lista dos domínios donos, cada um com o número de segredos, e o grafo do domínio escolhido: o domínio no centro e uma aresta para cada segredo que tem esse domínio como dono. Cada texto de domínio MUST ser um item próprio da lista. Esc MUST voltar à seção "Cofre".

#### Scenario: Domínio com dois segredos
- **WHEN** o cofre tem `openai-pessoal` e `openai-trabalho` com o domínio `api.openai.com` e o usuário abre o grafo e escolhe `api.openai.com`
- **THEN** a lista mostra `api.openai.com` com `2`, e o grafo mostra `api.openai.com` no centro ligado a `openai-pessoal` e a `openai-trabalho`

#### Scenario: Padrões diferentes
- **WHEN** um segredo tem o domínio `github.com` e outro tem `*.github.com`
- **THEN** a lista tem os dois domínios, cada um com o seu segredo

#### Scenario: Voltar
- **WHEN** o usuário aperta Esc com o foco na lista de domínios
- **THEN** a tela do grafo fecha, e a seção "Cofre" aparece como antes

### Requirement: Segredos de vários domínios e sem domínio no grafo
Um segredo com mais de um domínio dono MUST aparecer no grafo de cada um deles, com o número dos outros donos ao lado do nome. Os segredos sem domínio dono MUST ficar num grupo "Sem domínio" no fim da lista, que só aparece quando há algum. Uma conexão de banco MUST NOT entrar nesse grupo nem no grafo de um domínio.

#### Scenario: Segredo em três domínios
- **WHEN** `gh-pessoal` tem os domínios `github.com`, `api.github.com` e `ghcr.io` e o usuário escolhe `ghcr.io`
- **THEN** o grafo de `ghcr.io` mostra `gh-pessoal +2`

#### Scenario: Segredo sem domínio
- **WHEN** o cofre tem `stripe-teste` sem domínio
- **THEN** a lista termina com o grupo "Sem domínio", e escolher o grupo mostra `stripe-teste`

#### Scenario: Todos com domínio
- **WHEN** todo segredo do cofre tem pelo menos um domínio
- **THEN** a lista não tem o grupo "Sem domínio"

### Requirement: Conexões de banco na tela do grafo
Quando o cofre tem conexões de banco, a lista MUST terminar com o grupo "Bancos". Escolher o grupo MUST mostrar as conexões, e os detalhes da conexão em foco MUST mostrar o tipo, o servidor, a porta, o banco e o usuário, nunca a senha. As teclas de edição MUST NOT mudar uma conexão de banco: a tela MUST avisar que as conexões se gerenciam por `sandmux db`.

#### Scenario: Grupo de bancos
- **WHEN** o cofre tem a conexão `loja`, um postgres em `db.exemplo.com`, e o usuário escolhe o grupo "Bancos"
- **THEN** a tela mostra `loja`, e os detalhes mostram postgres, `db.exemplo.com`, a porta, o banco e o usuário, sem a senha

#### Scenario: Sem conexões
- **WHEN** o cofre não tem nenhuma conexão de banco
- **THEN** a lista não tem o grupo "Bancos"

#### Scenario: Conexão fora de "Sem domínio"
- **WHEN** o cofre tem a conexão `loja` e todo segredo tem domínio
- **THEN** a lista não tem o grupo "Sem domínio"

#### Scenario: Tecla de edição numa conexão
- **WHEN** com a conexão `loja` em foco, o usuário aperta `x`
- **THEN** a tela avisa que as conexões se gerenciam por `sandmux db`, e `loja` continua no cofre

### Requirement: Detalhes do segredo sem o valor
A tela do grafo MUST mostrar, para o segredo em foco, o nome, o tipo (token ou chave SSH), o valor falso, as ferramentas e todos os domínios donos; para uma chave SSH, a impressão digital no lugar do valor falso. O valor de um segredo MUST NOT aparecer em nenhuma parte da tela.

#### Scenario: Detalhes de um token
- **WHEN** o usuário põe o foco em `gh-pessoal`, um token com as ferramentas `gh` e `git` e três domínios
- **THEN** os detalhes mostram o tipo token, o valor falso, `gh git` e os três domínios

#### Scenario: Valor fora da tela
- **WHEN** o cofre tem `openrouter` com o valor `sk-teste-123` e o usuário percorre a tela do grafo
- **THEN** `sk-teste-123` não aparece na tela em nenhum momento

### Requirement: Grafo com o cofre trancado ou ausente
Sem cofre, a tecla `g` MUST mostrar na seção "Cofre" o aviso de que não há cofre, sem abrir a tela. Com o cofre trancado, a tela MUST abrir dizendo que o cofre está trancado e oferecer o desbloqueio pela tecla `u`; depois da senha certa, o grafo MUST aparecer.

#### Scenario: Desbloquear pela tela
- **WHEN** o cofre está trancado, o usuário abre a tela do grafo, aperta `u` e digita a senha certa
- **THEN** a lista de domínios e o grafo aparecem

#### Scenario: Sem cofre
- **WHEN** não há cofre e o usuário aperta `g` na seção "Cofre"
- **THEN** a seção mostra o aviso de que não há cofre, e a tela do grafo não abre

### Requirement: Edição do cofre pela tela do grafo
Na tela do grafo, com o cofre aberto, o usuário MUST poder, pelas teclas e formulários da seção "Cofre": acrescentar um segredo (`a`) ou uma chave SSH (`s`), com o domínio escolhido já preenchido; trocar os domínios (`d`) ou o valor (`v`) do segredo em foco; e apagar o segredo em foco (`x`), depois de confirmar. Senhas e valores MUST aparecer só como `•`. Depois de cada mudança, a tela MUST reler o cofre e redesenhar o grafo.

#### Scenario: Acrescentar no domínio escolhido
- **WHEN** com `gitlab.com` escolhido, o usuário aperta `a` e preenche o nome `gitlab-ci` e o valor, sem mexer nos domínios
- **THEN** `gitlab-ci` aparece no grafo de `gitlab.com`, o valor não aparece na tela, e `sandmux vault list` mostra `gitlab-ci` com `gitlab.com`

#### Scenario: Tirar um domínio
- **WHEN** com `ghcr.io` escolhido e `gh-pessoal` em foco, o usuário aperta `d` e tira `ghcr.io` dos domínios
- **THEN** `gh-pessoal` some do grafo de `ghcr.io` e continua no de `github.com`

#### Scenario: Apagar com confirmação
- **WHEN** o usuário põe o foco em `npm-token`, aperta `x` e confirma
- **THEN** `npm-token` some do grafo e do cofre, e o domínio que só tinha esse segredo some da lista
