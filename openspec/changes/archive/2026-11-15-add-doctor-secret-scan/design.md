## Context

A importação (change 1) troca as credenciais dos arquivos de um harness por valores falsos, com `secrets::scrub`, e anota as trocas num manifesto. `sandmux vault import` liga cada falso ao valor real, que o usuário cola. Depois da importação, nada olha o HOME sintético de novo, e uma chave gravada lá dentro pelo próprio harness fica em claro.

## Goals / Non-Goals

**Goals:**
- Encontrar chaves reais nos HOMEs sintéticos e levá-las ao cofre sem que o usuário as cole de novo.

**Non-Goals:**
- Procurar em toda a pasta de trabalho ou em arquivos fora da tabela de importação.
- Rodar a busca a cada lançamento. Fica no `doctor`, que o usuário roda.

## Decisions

### O valor real na troca

`Replacement` ganha `real: String`: o segredo sem o esquema (`Bearer`). Seu `Debug` mostra `real: "<oculto>"`. A importação ignora o campo, como antes.

### Busca

`sandmux_import::find_real_secrets(harness, home)` percorre os arquivos que a tabela do harness pega dentro do HOME sintético e aplica o `scrub` em memória. Para cada arquivo com trocas, devolve:
- o caminho;
- o texto já com os falsos;
- as trocas.

Nada é gravado nesse passo.

### `--fix`

Para cada arquivo:
1. pergunta (com terminal), mostrando quantas chaves e quais;
2. para cada troca:
   - o provedor vem do objeto em volta da linha (`vaultcmd::provider_of`, o mesmo do `vault import`);
   - o domínio vem de `known_domains` da chave ou do provedor, ou é perguntado;
   - `VaultAdd` com o falso daquela troca.
3. se todas entraram, grava o texto com os falsos, com o mesmo modo do arquivo, num temporário renomeado, e acrescenta as trocas ao manifesto do harness;
4. se alguma falhou, o arquivo não muda, e as que já entraram ficam no cofre. Elas não fazem mal, porque o falso não está em nenhum arquivo.

**Cofre:**
- **ausente:** o `--fix` para e indica `sandmux vault init`;
- **trancado:** o `--fix` pede a senha, como o `vault unlock`.

### Sessão viva

O harness pode ter a chave na memória. O `--fix` lembra de reiniciar as abas do harness.

## Risks / Trade-offs

- **[Falso-positivo]** Um valor que parece segredo, mas não é, vai para o cofre e é trocado no arquivo. O gateway devolve o real no tráfego ao domínio dono, e sem domínio o harness passa a usar o falso. A lista mostra a chave antes de confirmar.
