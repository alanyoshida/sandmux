# Tasks

## 1. Preparação

- [x] 1.1 Acrescentar `toml_edit` às dependências do workspace e do `sandmux-policy`, e `config_dir` ao `Host` (`$XDG_CONFIG_HOME/sandmux` ou `~/.config/sandmux`, ignorando valor relativo); verificar com teste de unidade do `Host` nos dois casos e com `cargo build`

## 2. Leitura e validação do arquivo

- [x] 2.1 Implementar `sandmux_policy::config` com `Config`, `Layer` e `Config::load`: arquivo ausente vira configuração vazia; seções `[global]`, `[profile.<nome>]` e `[project."<pasta>"]`; chaves `ro`, `rw`, `pass-env`, `repo`, `kvm`, `gpu` e `clipboard`; verificar com testes de unidade de cada seção e chave aceitas
- [x] 2.2 Recusar, com o arquivo, a linha e o motivo, os casos inválidos:
  - TOML malformado;
  - seção ou chave desconhecida;
  - valor de tipo errado;
  - caminho relativo e projeto com caminho relativo;
  - nome de variável inválido (inclusive `NOME=valor`);
  - nome de perfil inválido;
  - o mesmo caminho em `ro` e `rw` da mesma seção.

  Verificar com um teste de unidade por caso, conferindo a linha na mensagem

## 3. Resolução das camadas

- [x] 3.1 Implementar `resolve` com a ordem baseline, preset, global, perfil nomeado, projeto e `run`:
  - nas chaves de liga e desliga, vence a última camada que define;
  - as listas se somam;
  - num caminho em `ro` e `rw` de camadas diferentes, decide a mais específica;
  - a origem é guardada por chave e por item.

  Verificar com testes de unidade dos cenários "Camadas de política", "Precedência nas chaves de liga e desliga" e "Listas acumuladas entre camadas"
- [x] 3.2 Escolher o perfil de projeto pela pasta canonizada, com o maior prefixo por componentes, e recusar perfil nomeado inexistente listando os existentes; verificar com testes de unidade de subpasta, projetos aninhados, `/proj` contra `/proj2`, link simbólico para a pasta e perfil inexistente
- [x] 3.3 Converter a `Effective` em `RunOptions` (com `~/` expandido) mais o `clipboard`, e fazer `compose` aplicar às listas vindas da configuração as mesmas verificações das opções (caminho inexistente, pasta perigosa, aviso de credencial); verificar com teste de unidade de `compose` com caminhos vindos de um perfil

## 4. Pasta de configuração fora do alcance de escrita

- [x] 4.1 Dar a `check_dangerous` o parâmetro de escrita e recusar, como pasta de trabalho e em `rw`, a pasta de configuração, uma pasta dentro dela ou uma que a contém, aceitando em `ro`; verificar com testes de unidade (`--rw ~/.config` recusado, `--ro ~/.config` aceito, pasta de trabalho dentro da configuração recusada)
- [x] 4.2 Acrescentar à suíte de fuga:
  - uma tentativa de gravar no arquivo de configuração do sandmux de dentro de um sandbox lançado com `--ro ~/.config`;
  - uma tentativa de fazer a política mudar com `.sandmux.toml` e `.config/sandmux/config.toml` plantados na pasta de trabalho.

  Verificar que o arquivo do host não muda e que o lançamento seguinte não monta nada a mais

## 5. Opções do `run` e protocolo

- [x] 5.1 Trocar em `LaunchOptions` os booleanos por `Option<bool>` (`gpu` no lugar de `no_gpu`), acrescentar `profile` e subir `PROTOCOL` para 2; verificar com os testes de serialização do `sandmux-proto`
- [x] 5.2 Acrescentar ao `run` `--profile`, `--no-kvm`, `--gpu`, `--no-allow-clipboard` e `--no-repo`, cada negativa em conflito com a positiva; verificar com teste de CLI que `--kvm --no-kvm` termina com erro e com o teste de ajuda
- [x] 5.3 Fazer o `run` sem terminal carregar o arquivo, resolver as camadas e lançar com o resultado; verificar com testes de integração sem terminal:
  - `kvm = true` no projeto dá `/dev/kvm`;
  - `gpu = false` no global tira a GPU;
  - `ro` no perfil nomeado monta a pasta;
  - arquivo inválido recusa com a linha;
  - perfil inexistente recusa com a lista.
- [x] 5.4 Fazer o `run` com terminal resolver as camadas antes de pedir a sessão (recusas, avisos e valores de `pass-env` da política efetiva) e enviar as opções dadas e o perfil; verificar com testes de PTY dos cenários "Configuração inválida" e "Perfil inexistente" de "Verificações do run antes de criar a sessão"

## 6. Daemon

- [x] 6.1 Acrescentar `config_dir` à saudação e recusar uma pasta de configuração diferente, mostrando as duas; verificar com teste de cliente e daemon com `XDG_CONFIG_HOME` diferentes, conferindo que as sessões continuam
- [x] 6.2 Fazer o daemon resolver as camadas na criação e no reinício, recusando com o erro de variável ausente quando a política pede um nome que o cliente não enviou; ligar o filtro de clipboard pela política efetiva. Verificar com testes:
  - sessão criada na pasta de um projeto com `clipboard = true` deixa passar o OSC 52;
  - o `restart` depois de tirar `kvm` do perfil relança sem `/dev/kvm`;
  - o `restart` com o perfil apagado termina com erro.
- [x] 6.3 Fazer o `sandmux restart` resolver as camadas no cliente para saber quais valores de `pass-env` enviar; verificar com teste de reinício de sessão cujo perfil tem `pass-env`, com e sem a variável no terminal
- [x] 6.4 Fazer a restauração resolver as camadas e deixar parada, com o motivo, a sessão cuja política tem `pass-env`, cujo perfil sumiu ou cuja configuração é inválida; verificar com testes de restauração dos três cenários novos de "Restauração depois de reboot", com o daemon continuando a rodar
- [x] 6.5 Gravar `sessions.json` com `"version": 2`, as opções dadas e o perfil, e converter o formato do change 2 na leitura; verificar com teste de unidade que lê uma lista antiga (`kvm`, `no_gpu` e `allow_clipboard` verdadeiros e falsos) e com teste de que a lista nova não contém as chaves do perfil
- [x] 6.6 Escrever `XDG_CONFIG_HOME` na unidade systemd quando o cliente o tem definido; verificar com teste de unidade de `unit_text` com e sem a variável

## 7. Comandos `sandmux config`

- [x] 7.1 Implementar `sandmux config show` (CHAVE, VALOR e ORIGEM, um item de lista por linha, "padrão" ou "modificada (…)"), com `--workdir` e `--profile`; verificar com testes de CLI sem arquivo, com chave vinda do projeto e com arquivo inválido
- [x] 7.2 Implementar a edição: `set` e `unset` (aceitando `true`, `false`, `sim` e `não`), e `add` e `remove`. Cada um exige o alvo `--global`, `--profile <nome>` ou `--project [<pasta>]`, valida o resultado e a pasta perigosa em `rw` antes de gravar, apaga a chave e a seção que ficaram vazias e grava de forma atômica. Um arquivo novo é criado com modo 600 e pasta 700. Verificar com testes de CLI:
  - cada cenário de "Alterar a configuração pela linha de comando";
  - cada cenário de "Só a diferença é gravada", com um comentário escrito à mão preservado byte a byte.
- [x] 7.3 Implementar `sandmux config list` (perfis e projetos com as chaves, e "(pasta não existe)"); verificar com teste de CLI dos dois cenários de "Listar perfis"

## 8. Diagnóstico e documentação

- [x] 8.1 Acrescentar ao `doctor` a seção "Configuração" (caminho, e se não existe, é válido com número de perfis e projetos, ou é inválido com a linha); verificar com teste de CLI do arquivo inválido e conferindo que o arquivo não muda
- [x] 8.2 Escrever `docs/config.md`, com:
  - o arquivo, as seções e as chaves;
  - as camadas e a precedência;
  - os perfis;
  - os comandos;
  - a regra da pasta de configuração;
  - a política resolvida a cada lançamento;
  - a volta para a versão anterior.

  Atualizar o README e o `docs/sessions.md` (protocolo 2, perfil na lista). Verificar com o teste de documentação, estendido para conferir as chaves e os comandos do `config` contra o código

## 9. Verificação final

- [x] 9.1 Rodar `./scripts/check.sh` (testes, clippy e fmt) sem falhas e sem daemon de teste sobrando
- [x] 9.2 Verificar à mão, com o `XDG_CONFIG_HOME` e o `XDG_DATA_HOME` temporários, uma sessão real do opencode na pasta de um projeto com perfil (`gpu = false`, `ro` de uma pasta de documentos): o `config show` mostra as origens, o harness sobe, não enxerga a GPU e lê a pasta; depois de `sandmux daemon restart` a sessão volta com a mesma política
