# Tasks

Os testes que montam o sandbox precisam de user namespace sem privilégio. No Ubuntu, isso exige o perfil AppArmor da tarefa 2.1 instalado antes de rodar a suíte num terminal comum.

## 1. Estrutura do projeto

- [x] 1.1 Criar o workspace Rust com as bibliotecas `sandmux-spec` (especificação de lançamento), `sandmux-launcher` (montagem do sandbox), `sandmux-policy` (baseline, presets, toolchains) e `sandmux-import`, e os binários `sandmux-run`, `sandmux` e `sandmux-probe` (sonda dos testes de fuga); verificar que `cargo build --workspace` termina sem erro
- [x] 1.2 Acrescentar as dependências (Landlock, filtro seccomp, chamadas de sistema, linha de comando, serialização) sem biblioteca C além da libc; verificar com `cargo tree` que nenhuma dependência compila código C
- [x] 1.3 Adicionar `.gitignore`, `LICENSE-MIT` e `LICENSE-APACHE`, e declarar a licença dupla nos manifestos; verificar que `cargo metadata` mostra `MIT OR Apache-2.0` em todos os pacotes
- [x] 1.4 Configurar `cargo fmt --check`, `cargo clippy -- -D warnings` e `cargo test --workspace` num script único de verificação; verificar que o script passa no workspace vazio

## 2. Provas de premissa

Cada prova termina com o resultado anotado na seção 6 do `openspec/roadmap.md`. Se uma premissa falhar, parar e revisar o `design.md` antes de seguir.

- [ ] 2.1 Escrever o perfil AppArmor do `sandmux-run` em `packaging/apparmor/` e, num terminal comum do Ubuntu 26.04, confirmar que um binário de teste monta um tmpfs dentro de um user namespace com o perfil instalado e falha sem ele; anotar o resultado e o comando de instalação
- [x] 2.2 Provar que um overlay montado em user namespace, com uma pasta do HOME real por baixo e a camada de escrita no diretório de dados, aceita `cargo install` e `npm install -g` sem alterar a pasta de baixo; verificar comparando a pasta do host antes e depois
- [x] 2.3 Provar que um arquivo coberto por montagem em cima do overlay não pode ser apagado, renomeado nem substituído de dentro do sandbox; verificar com as três tentativas falhando
- [x] 2.4 Provar que um diretório vazio pré-autorizado no Landlock dá acesso a uma pasta montada sob ele depois que a restrição foi aplicada; anotar se a premissa da decisão 10 do design vale
- [x] 2.5 Confirmar que `updates.check: false` no `config.yaml` desliga a checagem de atualização do hermes; verificar que o hermes iniciado com a chave não faz a consulta ao GitHub
- [x] 2.6 Descobrir se o opencode usa a chave de `OPENROUTER_API_KEY` quando o `auth.json` tem um valor falso para o mesmo provedor; anotar o resultado e, se o arquivo vencer, definir como o `--pass-env` resolve o conflito
- [x] 2.7 Medir se o overlay continua íntegro quando o cache do host muda com o sandbox montado (instalar um pacote no host durante uma instalação no sandbox, com npm e com cargo); anotar se há corrupção

## 3. Especificação de lançamento

- [x] 3.1 Definir em `sandmux-spec` os tipos da especificação versionada (montagens de leitura, de escrita, de overlay, de cobertura e tmpfs; regras de Landlock; dispositivos; ambiente; comando e pasta inicial) com leitura de JSON que recusa campo desconhecido; verificar com testes de ida e volta e de recusa de campo desconhecido e de versão não suportada
- [x] 3.2 Fazer o `sandmux-run` ler a especificação de um descritor de arquivo herdado e terminar com erro claro para entrada malformada; verificar com testes de integração que nenhum comando é executado nos três casos de erro da spec `sandbox-isolation`
- [x] 3.3 Documentar o formato da especificação em `docs/launch-spec.md`, com um exemplo completo; verificar que o exemplo é aceito pelo teste de leitura

## 4. Lançador do sandbox

- [x] 4.1 Implementar no `sandmux-probe` as sondagens de processo, socket (por caminho e abstrato), identidade, chamadas negadas, injeção de teclas, ambiente e escrita em caminho arbitrário, com saída em JSON; verificar que, rodada fora do sandbox, a sonda reporta como bem-sucedidas as tentativas que o host permite (as chamadas privilegiadas, como `mount`, falham fora com outro erro que não EPERM, o que distingue o filtro)
- [x] 4.2 Implementar as verificações de suporte (user namespace sem privilégio, versão do Landlock, overlay em user namespace) e a recusa do lançamento; verificar com teste que simula cada falta e confere que o comando não roda
- [x] 4.3 Criar os namespaces (user, mount, pid, ipc, uts) com o UID e o GID mapeados para eles mesmos, e o processo 1 que recolhe órfãos e repassa sinais; verificar pela sonda que só os processos do sandbox aparecem e que um órfão é recolhido
- [x] 4.4 Montar a nova raiz em tmpfs, aplicar as montagens da especificação (leitura, escrita, overlay, cobertura, tmpfs), montar `/proc`, `/dev`, `/tmp`, `/run` e `/var/tmp` novos e trocar de raiz; verificar pela sonda que um caminho do host fora da especificação não existe
- [x] 4.5 Descartar as capabilities, ligar `no_new_privs` e aplicar o Landlock com as regras de arquivo e o escopo de sockets abstratos e de sinais; verificar pela sonda que a conexão a um socket abstrato do host falha, que um sinal para o host falha e que `sudo` não eleva
- [x] 4.6 Aplicar o filtro seccomp por lista de negação, devolvendo erro de permissão, incluindo os controles de terminal que injetam teclas; verificar pela sonda que cada chamada da lista devolve erro sem matar o processo e que `ptrace` entre processos do sandbox funciona
- [x] 4.7 Executar o comando com o ambiente da especificação, entregar o terminal em primeiro plano, repassar sinais e propagar o código de saída, encerrando o que restar no sandbox; verificar com testes de código de saída, de sinal de término e de processo em segundo plano
- [x] 4.8 Verificar que um user namespace aninhado é criado dentro do sandbox e que uma montagem dentro dele falha; cobrir com um caso na sonda

## 5. Baseline e visão de arquivos

- [x] 5.1 Resolver a pasta de trabalho (pasta atual ou `--workdir`) e recusar o HOME, a raiz, pastas que contêm o HOME e pastas do diretório de dados, também para `--ro` e `--rw`; verificar com testes de cada caso da spec `sandbox-filesystem`
- [x] 5.2 Criar o diretório de dados (`homes/`, `layers/`, `inbox/`, `manifests/`) e o HOME sintético por harness, montado no caminho do HOME real; verificar pela sonda que `~/.ssh` não existe e que um arquivo gravado no HOME persiste entre execuções
- [x] 5.3 Compor as montagens de sistema em somente leitura e gerar `/etc/passwd` e `/etc/group` só com o usuário, `root` e `nobody`; verificar pela sonda que a escrita em `/usr` e `/etc` falha e que o nome do usuário é resolvido
- [x] 5.4 Embutir as três listas de `/etc` cobertos da spec e aplicar a cobertura só ao que existe na máquina; verificar pela sonda que cada caminho existente aparece vazio e que a validação de TLS por `/etc/ssl/certs` funciona
- [x] 5.5 Resolver o destino de `/etc/resolv.conf` fora do sandbox e montar o arquivo no caminho de destino; verificar que um nome público é resolvido de dentro do sandbox
- [x] 5.6 Compor o `/dev` mínimo e detectar os dispositivos de GPU (nós de renderização e nós da NVIDIA, sem o de controle de modo de vídeo), com `--no-gpu` e `--kvm`; verificar pela sonda a lista de `/dev` nos três modos e, na máquina de referência, que `nvidia-smi` enxerga a GPU dentro do sandbox
- [x] 5.7 Montar a pasta de entrada em somente leitura em `~/inbox` e definir `SANDMUX_INBOX`; verificar que um arquivo copiado no host é lido lá dentro e que a escrita falha
- [x] 5.8 Implementar `--ro` e `--rw` e o aviso de credencial conhecida, com a lista da spec embutida; verificar com testes de pasta comum, credencial, pasta que contém credencial e caminho inexistente
- [x] 5.9 Construir o ambiente a partir da lista segura e implementar `--pass-env` com o aviso sem valores; verificar pela sonda que uma variável exportada no terminal não entra e que a passada entra

## 6. Toolchains e caches

- [x] 6.1 Embutir a lista de toolchains (nvm, bun, cargo, rustup, uv, pyenv, go, linuxbrew) com local, pastas de `PATH` e arquivos cobertos, ignorando as ausentes, e reconstruir o `PATH`; verificar com testes sobre um HOME de exemplo e, na máquina de referência, que `cargo`, `node`, `bun`, `uv`, `python` do pyenv e `go` rodam dentro do sandbox
- [x] 6.2 Montar cada pasta de toolchain e cada cache conhecido como overlay sobre a camada do harness em `layers/<harness>/`; verificar que `cargo install`, `npm install -g`, `rustup component add` e `go install` funcionam e que a pasta do host fica idêntica
- [x] 6.3 Cobrir os arquivos de credencial das toolchains e os tokens do cache do Hugging Face por cima do overlay; verificar pela sonda que cada um aparece vazio e não pode ser apagado nem renomeado
- [x] 6.4 Implementar a trava da camada de escrita e a camada temporária com aviso para a segunda execução simultânea; verificar com duas execuções ao mesmo tempo que a segunda inicia e que a camada temporária é apagada ao sair
- [x] 6.5 Implementar o recuo sem overlay (caches próprios vazios e toolchains em somente leitura, com aviso); verificar forçando a falha do overlay num teste e conferindo que nenhuma pasta do host é entregue com escrita

## 7. Git da pasta de trabalho

- [x] 7.1 Montar `.git/hooks` e `.git/config` em somente leitura, inclusive nos diretórios git de submódulos em `.git/modules`; verificar que commit, branch, merge e stash funcionam e que criar um hook, `git config` local e `git remote add` falham
- [x] 7.2 Detectar worktree e montar o diretório git do repositório principal com a mesma proteção; verificar que um commit num worktree funciona e que criar um hook no repositório principal falha
- [x] 7.3 Detectar subpasta de repositório, avisar por padrão e implementar `--repo` (repositório em somente leitura, subpasta e `.git` graváveis, hooks e configuração protegidos); verificar os dois cenários da spec
- [x] 7.4 Gravar a configuração mínima do git ao criar o HOME sintético, só com nome e e-mail, sem regravar depois; verificar o autor de um commit feito no sandbox, o caso sem identidade e a preservação de um ajuste manual
- [x] 7.5 Listar as operações comuns do git que esbarram na proteção (por exemplo, criar uma branch que acompanha uma remota) e documentá-las em `docs/git.md`; verificar cada item da lista rodando o comando dentro do sandbox

## 8. Presets dos harnesses

- [x] 8.1 Definir o formato interno de preset (como localizar, código em somente leitura, pastas pela camada de escrita, variáveis, comando) e a recusa para nome desconhecido e para harness não instalado, com a instrução de instalação; verificar com testes dos cenários da spec `harness-presets`
- [x] 8.2 Implementar o preset `shell` (shell do usuário se estiver no sistema, senão `bash`; arquivo de configuração mínimo gerado ao criar o HOME sintético) e o repasse dos argumentos depois de `--`; verificar que `sandmux run shell -- -c "id -u"` devolve o UID do usuário e o código de saída
- [x] 8.3 Implementar o preset do opencode (`~/.opencode` em somente leitura, estado no HOME sintético, `OPENCODE_DISABLE_AUTOUPDATE`); verificar que `sandmux run opencode -- --version` mostra a versão e que a interface abre na pasta de trabalho
- [ ] 8.4 Implementar o preset do hermes (código e os três comandos em somente leitura, `~/.hermes/tools` pela camada de escrita, `updates.check: false`, gateway próprio); verificar que a interface abre, que o `gateway.sock` do host não existe lá dentro e que o gateway do sandbox termina junto com o hermes
- [x] 8.5 Verificar pela sonda que o código de cada harness não pode ser alterado de dentro do sandbox e que o backend Docker do hermes falha sem alcançar o Docker do host
- [x] 8.6 Gerar, com `--pass-env` no preset do hermes, o `.env` da execução (linhas do `.env` sintético, valores reais no lugar dos falsos, recusa de quebra de linha e `${`) e montá-lo em somente leitura sobre `~/.hermes/.env`; verificar com testes que o hermes lê o valor real pelo próprio parser, que o arquivo não pode ser alterado de dentro e que o `.env` sintético no host continua com o valor falso

## 9. Importação

- [x] 9.1 Implementar a cópia guiada por tabela de permissão, com as tabelas do opencode e do hermes da spec `harness-state-import`, deixando de fora o que não está listado; verificar com árvores de exemplo que histórico, logs, caches e arquivos desconhecidos não são copiados
- [x] 9.2 Implementar a troca de segredos em JSON, YAML e `.env` (nome de chave de credencial ou valor com formato de token; na dúvida, troca), com valores `sandmux-fake-` únicos; verificar com arquivos de exemplo que nenhum valor de credencial de entrada aparece na saída e que a estrutura é mantida
- [x] 9.3 Gravar o manifesto em `manifests/` sem valores reais e mostrar o relatório do que foi copiado, ignorado e trocado; verificar que o manifesto e a saída do comando não contêm nenhum valor de entrada
- [x] 9.4 Implementar a reimportação (só arquivos novos, lista de diferenças) e `--overwrite`; verificar os quatro cenários de reimportação da spec
- [x] 9.5 Oferecer a importação na primeira execução do `run`, lembrar a recusa, e exigir `--import` ou `--no-import` sem terminal interativo; verificar os cenários de aceite, recusa, nada a importar e execução por script
- [x] 9.6 Rodar `sandmux import` contra o estado real do opencode e do hermes na máquina de referência e revisar o relatório à mão; verificar com uma busca que nenhum valor de credencial real ficou no HOME sintético

## 10. Linha de comando e diagnóstico

- [x] 10.1 Implementar `sandmux run` ligando baseline, preset e opções à especificação e chamando o `sandmux-run`, com a ajuda de cada opção; verificar que `sandmux run --help` lista `--workdir`, `--ro`, `--rw`, `--pass-env`, `--import`, `--no-import`, `--repo`, `--kvm` e `--no-gpu`
- [x] 10.2 Implementar `sandmux doctor` com cada verificação, a causa e a correção, o texto do perfil AppArmor e o comando de instalação, o caminho da pasta de entrada e o aviso de rede sem filtro; verificar os três cenários de diagnóstico da spec e o código de saída
- [x] 10.3 Atualizar o `README.md` com instalação a partir do código, os três comandos, as opções e as limitações desta fatia; verificar que cada comando do README roda como escrito

## 11. Integração

- [x] 11.1 Rodar a suíte completa de fuga dentro de um sandbox lançado por `sandmux run`, cobrindo cada requisito de `sandbox-isolation` e `sandbox-filesystem`; verificar que todas as tentativas falham e que o resultado sai num relatório único
- [ ] 11.2 Subir o opencode e o hermes reais com `--pass-env`, pedir a cada um que crie um arquivo, faça um commit e instale uma ferramenta global; verificar que o arquivo e o commit aparecem no host e que as toolchains do host ficam idênticas
- [x] 11.3 Iniciar um navegador headless dentro do sandbox; verificar que ele abre uma página e anotar o resultado na seção 6 do roadmap
- [x] 11.4 Rodar `openspec validate add-sandbox-cli --strict` e atualizar a situação do change e as verificações no `openspec/roadmap.md`; verificar que a validação passa e que o roadmap reflete o que foi medido
