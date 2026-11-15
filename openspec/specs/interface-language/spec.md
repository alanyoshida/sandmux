# interface-language Specification

## Purpose
O idioma em que o sandmux fala com o usuário: português ou inglês, escolhido na configuração, pela variável `SANDMUX_LANG` ou pelo sistema.

## Requirements

### Requirement: Idioma da interface
O sandmux MUST mostrar todo texto ao usuário em português ou em inglês: a TUI, a saída e os erros da linha de comando, os avisos do daemon e do supervisor e as mensagens de validação da configuração. Os nomes das chaves, seções e valores do arquivo de configuração, dos subcomandos e das opções MUST continuar os mesmos nos dois idiomas.

#### Scenario: Linha de comando em inglês
- **WHEN** o idioma é inglês e o usuário roda `sandmux config set kvm talvez --global`
- **THEN** o erro aparece em inglês, e a chave continua `kvm`

### Requirement: Escolha do idioma
O idioma MUST vir, nesta ordem:
- de `SANDMUX_LANG` (`pt` ou `en`);
- da chave `language` de `[global]` (`"pt"` ou `"en"`, aceita só em `[global]`);
- do sistema (`LC_ALL`, `LC_MESSAGES`, `LANG`), em que um valor começado por `pt` é português e qualquer outro, inclusive vazio ou `C`, é inglês.

Um valor de `language` diferente de `"pt"` e `"en"` MUST tornar o arquivo inválido.

#### Scenario: Seguir o sistema
- **WHEN** não há `language` nem `SANDMUX_LANG`, e `LANG=en_US.UTF-8`
- **THEN** o sandmux fala inglês

#### Scenario: Configuração passa por cima do sistema
- **WHEN** `[global]` tem `language = "pt"` e `LANG=en_US.UTF-8`
- **THEN** o sandmux fala português

### Requirement: Idioma na página de configurações
A página de configurações MUST ter, na seção "Geral", o item "Idioma" com `auto`, `pt` e `en`. `auto` MUST apagar a chave `language`, e a TUI MUST passar a mostrar o idioma novo na hora.

#### Scenario: Trocar para inglês
- **WHEN** o usuário passa "Idioma" para `en`
- **THEN** o `config.toml` tem `language = "en"` em `[global]`, e a página, a barra e o painel aparecem em inglês

### Requirement: Documentação em inglês
Cada arquivo de `docs/` MUST ter a versão em inglês em `docs/en/` com o mesmo nome, e o README MUST ter `README.en.md`, com links entre as duas versões. Os exemplos de configuração da versão em inglês MUST ser válidos.

#### Scenario: Par de cada doc
- **WHEN** existe `docs/network.md`
- **THEN** existe `docs/en/network.md`, e o README em português aponta para o `README.en.md`
