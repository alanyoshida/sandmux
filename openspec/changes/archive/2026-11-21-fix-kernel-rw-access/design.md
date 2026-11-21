## Context

O motivo está em proposal.md, em "Why". O que importa para o desenho:

- O gancho `file_open` (`crates/sandmux-ebpf/src/prog.rs`) lê o `f_mode` do arquivo. Para escolher as marcas que valem, ele testa só o `FMODE_WRITE` (0x2). O evento de abertura grava `f_mode & FMODE_WRITE` no campo de 4 bytes do deslocamento 12, e a chave do mapa das repetições guarda o mesmo valor.
- O evento de bloqueio grava nesse campo uma constante, 1 numa operação de escrita e 0 numa leitura, também no `file_open`.
- O `ring::Event::decode` lê o campo como `write = campo != 0`. O auxiliar manda `write` no JSON, e o daemon (`daemon/dockerfiles.rs`, `entries`) faz `w` ou `r`.
- O auxiliar e os programas BPF saem juntos, no mesmo binário. O que precisa de compatibilidade é o JSON entre o auxiliar e o daemon, que podem ser de versões diferentes.

## Goals / Non-Goals

**Goals:**

- Uma abertura com `O_RDWR` aparece como `rw` no registro, numa aba pelo kernel, como numa aba pelo supervisor, aberta ou bloqueada.
- Uma mistura de versões entre o daemon e o auxiliar não quebra o registro: no pior caso, ele fica como hoje.

**Non-Goals:**

- Mudar quando uma abertura é negada: as marcas de escrita continuam escolhidas pelo `FMODE_WRITE`.
- Mudar o registro das criações: a criação continua vindo do evento de criação.

## Decisions

### 1. O campo do deslocamento 12 passa a ter os bits do `f_mode`

O campo guarda `f_mode & (FMODE_READ | FMODE_WRITE)` nos eventos de abertura e no bloqueio do `file_open`. Nos outros bloqueios, que são de escrita (criar, apagar, renomear, ligar, mudar atributos), ele guarda `FMODE_WRITE` como constante. O `decode` lê `read = campo & 1` e `write = campo & 2`.

- **Alternativa:** um campo novo para a leitura. O cabeçalho de 32 bytes não tem espaço livre, e crescê-lo muda o tamanho de todos os eventos e o `EV_HEADER` do change 80. Os dois bits já cabem no campo de hoje.
- **No bloqueio do `file_open`**, o modo é lido de novo do arquivo em R8, depois da busca da marca: R8 é preservado entre as chamadas, e R1 e R2 não.

### 2. O mapa das repetições separa os três modos

A chave guarda `f_mode & 3`. Assim, ler um arquivo e depois abri-lo para leitura e escrita dá duas linhas (`r` e `rw`), como no supervisor. O `forget_opened` apaga as quatro chaves possíveis (0 a 3): o valor 0 é raro, mas um apagamento a mais não custa nada.

### 3. `read` novo no JSON, `write` como hoje

O auxiliar manda `"read"` além de `"write"`. O daemon decide assim: com `write`, `rw` se `read` for verdadeiro e `w` se não; sem `write`, `r`. Um auxiliar antigo não manda `read`, e as aberturas de escrita dele continuam como `w`. Um daemon antigo ignora o campo novo.

## Risks / Trade-offs

- **O mapa das repetições ganha até uma entrada a mais por arquivo** (os três modos). → O LRU tem 16384 entradas; uma expulsa só faz a próxima abertura mandar evento, e o daemon tira a repetição.
- **Um auxiliar antigo com o daemon novo** continua registrando `w` até `sandmux setup`. → A nota da versão pede para rodar o `setup`, e o `doctor` já avisa do auxiliar desatualizado.

## Migration Plan

Atualizar o sandmux e rodar `sandmux setup`, que atualiza o auxiliar. As abas em execução pegam os programas novos no próximo confinamento (relançadas ou numa recuperação). Para desfazer, basta voltar a versão anterior.
