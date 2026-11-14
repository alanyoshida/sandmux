# live-folders Specification

## Purpose
Define como uma pasta do host entra no sandbox com o agente rodando: o lugar `/mnt/sandmux`, montar e desmontar ao vivo, o aviso ao agente, as pastas lembradas na aba e a tela de pastas.

## Requirements

### Requirement: Lugar das pastas novas
Todo sandbox MUST ter `/mnt/sandmux`, liberado para escrita no Landlock desde o início, e a variável `SANDMUX_FOLDERS=/mnt/sandmux`. `/mnt/sandmux/PASTAS.txt` MUST listar, em somente leitura, cada pasta montada com o nome, a pasta do host e o modo.

#### Scenario: Sandbox novo
- **WHEN** o agente roda `ls /mnt/sandmux` num sandbox sem pastas
- **THEN** a pasta existe e só tem o `PASTAS.txt`

### Requirement: Montar uma pasta ao vivo
`sandmux folder add <id> <pasta> [--rw]` MUST montar a pasta do host em `/mnt/sandmux/<nome>` na sessão viva, sem relançar o agente, em somente leitura ou, com `--rw`, com escrita. Ler, gravar, criar, apagar e renomear MUST valer na hora, conforme o modo. O nome MUST ser o último componente da pasta, com `-2`, `-3` quando repete. `sandmux folder remove <id> <nome>` MUST desmontar, e `sandmux folder list <id>` MUST listar.

#### Scenario: Pasta com escrita
- **WHEN** a sessão 1 está viva e o usuário roda `sandmux folder add 1 ~/dados --rw`
- **THEN** o agente lê, cria, renomeia e apaga arquivos em `/mnt/sandmux/dados`, e as mudanças aparecem em `~/dados` no host

#### Scenario: Pasta só de leitura
- **WHEN** o usuário roda `sandmux folder add 1 ~/docs`
- **THEN** o agente lê `/mnt/sandmux/docs` e não consegue gravar nela

#### Scenario: Desmontar
- **WHEN** o usuário roda `sandmux folder remove 1 dados`
- **THEN** `/mnt/sandmux/dados` some do sandbox

### Requirement: Pasta perigosa recusada
Montar ao vivo o HOME, a raiz, uma pasta que contém o HOME ou o diretório de dados do sandmux MUST ser recusado, e também, com escrita, a pasta de configuração do sandmux, como no `--rw`.

#### Scenario: Pasta perigosa
- **WHEN** o usuário roda `sandmux folder add 1 ~`
- **THEN** o comando é recusado com o motivo

### Requirement: Pastas lembradas na aba
As pastas montadas numa sessão MUST voltar nos lançamentos seguintes dela, no mesmo `/mnt/sandmux/<nome>` e com o mesmo modo. Uma pasta adicionada a uma sessão parada MUST valer no próximo lançamento.

#### Scenario: Relançar
- **WHEN** a sessão 1 tem a pasta `dados` e é relançada com `sandmux restart 1`
- **THEN** `/mnt/sandmux/dados` está montada no lançamento novo

### Requirement: Tela de pastas
Na TUI, Ctrl+b `o` MUST abrir a tela de pastas da aba ativa, com o nome, a pasta do host e o modo de cada uma. `a` MUST pedir uma pasta e o modo (`r` ou `w`) e montá-la, e `x` MUST desmontar a escolhida.

#### Scenario: Montar pela tela
- **WHEN** o usuário abre a tela, aperta `a`, digita uma pasta, Enter e `r`
- **THEN** a pasta aparece na lista e em `/mnt/sandmux` no sandbox
