## Why

O supervisor de arquivos atende cada `open`, `openat`, `openat2` e `creat` do agente: ele lê o caminho, abre o arquivo dentro do sandbox, decide, registra e entrega o descritor. Isso custa de 3,1x a 3,5x na leitura em massa (medido no change 15). O supervisor também precisa de emulação, como a tradução do `/proc/self`, os namespaces aninhados e o `lsof` limitado. É o terceiro item que o usuário pediu na etapa 16, e a lacuna "A interceptação de arquivos" da seção 5 do roadmap.

O confinamento por BPF LSM dos changes 70 e 71 já faz no kernel, para o Docker privado, o que o supervisor faz por caminho:
- nega pelo arquivo os segredos e as pastas negadas;
- registra os bloqueios e as criações;
- acompanha ao vivo a pasta e a política.

Decisões do usuário (2026-11-20):
- **o kernel decide e registra**:
  - os segredos e o `file-deny` são negados pelo arquivo, com os mesmos erros de hoje (`ENOENT` e `EACCES`);
  - cada abertura nas pastas do agente vai ao registro pela fila do kernel, com o caminho;
  - o supervisor deixa de atender as aberturas.
- **automático, com recuo:** com o auxiliar root, o BPF LSM e o escopo próprio da aba, toda aba usa o kernel; sem algum deles, o supervisor fica como hoje.
- **a janela aceita:** um segredo criado no host com a sessão aberta só fica negado depois de marcado, em até ~1 s, como no Docker. Um segredo que o próprio agente criou continua legível.

## What Changes

- **O daemon confina a aba** a pedido do lançador, antes de o comando rodar:
  - pede o `confine` do escopo da aba ao auxiliar;
  - marca os segredos, o `file-deny` (com o que existe dentro das pastas negadas) e o `.git`;
  - liga a mesma vigia ao vivo do Docker confinado.

  O lançador espera a resposta: com o confinamento, o comando roda sem o supervisor de aberturas; sem ele, roda com o supervisor, como hoje. Quando a conexão com o auxiliar cai, a aba fica congelada (`cgroup.freeze`) até o daemon refazer o confinamento com o auxiliar de volta; sem ele em 10 s, a aba para (decisão do usuário, 2026-11-20: uma atualização ou um reinício do auxiliar não mata as sessões dos agentes).
- **Os programas BPF** ganham:
  - duas marcas novas, segredo (`ENOENT`) e negado pela política (`EACCES`), ao lado das do Docker, que não mudam;
  - o evento de abertura, com o caminho (`bpf_d_path`) e se a abertura grava, só para os arquivos das montagens que o registro acompanha.
- **O registro de arquivos** das abas pelo kernel tem as mesmas linhas do supervisor. O daemon as grava a partir dos eventos.
- **`sandmux stats`, o painel e o `doctor`** dizem de onde vêm a decisão e o registro dos arquivos da aba: o kernel ou o supervisor.
- **Medida:** a leitura em massa e o import do Python, com e sem o caminho do kernel, na VM.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `file-access-log`: as aberturas das abas decididas e registradas pelo kernel quando há o auxiliar, com o supervisor como recuo.
- `file-policy`: a política de arquivos pelo kernel, com a janela dos segredos criados no host.
- `root-helper`: os eventos de abertura do confinamento.

## Impact

- `crates/sandmux-ebpf`: as marcas novas, o evento de abertura com o caminho e o filtro pelas montagens registradas.
- `crates/sandmux/src/helper.rs`: as opções novas do `confine` (as montagens registradas e as marcas das abas).
- `crates/sandmux/src/daemon`:
  - o confinamento da aba;
  - o `dockerfiles::Files` generalizado para as abas;
  - as linhas do registro a partir dos eventos de abertura.
- `crates/sandmux-proto`: o pedido do lançador ao daemon. A versão do protocolo sobe.
- `crates/sandmux-launcher`: o pedido antes de liberar o comando, e o modo no byte de sincronização com o filho.
- `docs/files.md`, `docs/helper.md` e os pares em inglês.
