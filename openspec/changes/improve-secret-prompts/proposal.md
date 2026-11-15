## Why

No primeiro uso do cofre, o usuário não entendeu a pergunta do `sandmux vault import` ("Authorization em .config/opencode/opencode.json (linha 10). Valor real") e não via nada ao digitar ou colar, sem saber se o texto entrava. Antes, ele tinha tentado a troca da chave sem cofre, e nada avisou que faltava criá-lo (2026-11-14).

## What Changes

- **Bolinhas ao digitar:** a leitura de segredos e senhas na linha de comando (`vault init`, `unlock`, `passwd`, `add`, `add-ssh`, `import`, `sandmux chat`) mostra `•` por caractere. Backspace apaga um, Ctrl+U apaga tudo, e sequências de escape (setas, marcadores de colagem) são ignoradas. O valor continua sem aparecer.
- **Pergunta do `vault import` com contexto:** diz que a importação trocou aquela chave por um falso e mostra a linha do arquivo importado, com o falso como `<falso>`, para o usuário reconhecer o provedor. Diz também que o valor não aparece e que Enter vazio pula.
- **Domínio pelo provedor:** quando o nome da chave não indica o dono (o `key` do `auth.json` do opencode), o `vault import` usa o nome do objeto em volta (`"openrouter": {`), mostra o provedor na pergunta e diz, ao guardar, com que domínio guardou, ou que guardou sem domínio e não haverá troca. No uso real, as duas chaves foram guardadas sem domínio e nada era trocado.
- **Nome com o provedor:** o segredo importado passa a se chamar `<harness>:<arquivo>:<provedor>.<chave>`. No uso real, o `key` do `lmstudio` e o do `openrouter` no `auth.json` tinham o mesmo nome, e o segundo sobrescreveu o primeiro.
- **Aviso de cofre ausente:** o relatório da importação, quando trocou segredos, mostra `sandmux vault init` e `sandmux vault import`. O `sandmux doctor` mostra o estado do cofre e avisa quando há chaves trocadas sem cofre.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `secret-vault`: a leitura do manifesto mostra o contexto e as bolinhas.

## Impact

- **`sandmux`:**
  - `vaultcmd.rs`: a leitura com bolinhas e o contexto do `import`;
  - `main.rs`: o relatório da importação;
  - `doctor.rs`: a seção do cofre.
- **Documentação:** `docs/vault.md`.
