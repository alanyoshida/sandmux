## Why

Dentro do sandbox, o "Connect provider" do opencode (e o assistente de configuração do hermes) grava a chave real no HOME sintético, ao alcance do agente. Foi o que aconteceu no primeiro uso real (2026-11-14): a chave do Unsloth Studio ficou no `auth.json` do sandbox. O sandmux não percebia nada disso. O usuário pediu que o `sandmux doctor` encontre essas chaves e as leve para o cofre.

## What Changes

- **Busca no `doctor`:** o `sandmux doctor` lê, nos HOMEs sintéticos do opencode e do hermes, os mesmos arquivos que a importação copia (`auth.json`, `opencode.json`, `.hermes/.env`, `config.yaml` e os outros da tabela), com as mesmas regras de chave de credencial e de valor de segredo. Lista cada chave real encontrada (harness, arquivo, provedor, chave e linha), sem o valor, e indica `sandmux doctor --fix`.
- **`sandmux doctor --fix`:** para cada arquivo com chaves reais:
  1. confirma com o usuário;
  2. guarda cada chave no cofre com um valor falso novo, com o nome `<harness>:<arquivo>:<provedor>.<chave>` e o domínio pelo provedor (ou perguntado, como no `vault import`);
  3. só então regrava o arquivo com os falsos e anota no manifesto da importação.

  Com o cofre ausente ou trancado, pede para criá-lo ou pede a senha. Se o cofre recusar uma chave, o arquivo não muda.
- **A troca segue sozinha:** depois do `--fix`, o harness usa o falso, e o gateway troca pelo real no tráfego, como nas chaves importadas.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `secret-vault`: o diagnóstico encontra chaves reais nos HOMEs sintéticos e as leva para o cofre.

## Impact

- **`sandmux-import`:** `secrets::Replacement` guarda o valor real, com `Debug` que o esconde; `find_real_secrets` e a regravação com os falsos.
- **`sandmux`:**
  - `doctor.rs`: a seção e o `--fix`;
  - `vaultcmd.rs`: o provedor em volta, compartilhado com o `vault import`;
  - `main.rs`: `doctor --fix`.
- **Documentação:** `docs/vault.md` e o README.
