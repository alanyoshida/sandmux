## Context

O protocolo já tem as operações do cofre:
- `VaultStatus`, `VaultInit`, `VaultUnlock`, `VaultLock` e `VaultPasswd`;
- `VaultAdd`, `VaultAddSsh`, `VaultRemove` e `VaultList` (`VaultEntries`).

Falta trocar os domínios sem o valor. A página de configurações (change 35) tem seções de itens do `config.toml`; o cofre não está no arquivo, então a seção dele é uma tela própria dentro da página.

## Goals / Non-Goals

**Goals:**
- Tudo do dia a dia do cofre sem sair da TUI.
- Nenhum valor na tela.

**Non-Goals:**
- Conexões de banco.
- A importação de chaves (`vault import`), que depende do terminal e do relatório da importação.
- Mostrar valores.

## Decisions

- **Seção:** "Cofre" é a última seção da página. Com ela escolhida, a área da direita mostra o estado, a lista e a linha de teclas.
- **Teclas:**

  | Tecla | Ação |
  |---|---|
  | `n` | Criar o cofre (senha e confirmação) |
  | `u` / `l` | Desbloquear / trancar |
  | `p` | Trocar a senha |
  | `a` | Acrescentar um segredo |
  | `s` | Acrescentar uma chave SSH |
  | `d` | Editar os domínios do escolhido |
  | `x` | Apagar o escolhido, com `s` para confirmar |

- **Formulário:** campos em sequência, um por vez, com Enter avançando e Esc cancelando. Os campos de senha e de valor mostram `•`; o texto deles fica num `Zeroizing<String>` e é apagado depois do envio.
- **Domínios:** separados por vírgula; vazio é recusado na criação de token (como no `vault add`, que exige o domínio para trocar a chave).
- **Chave SSH:** o arquivo é lido pela TUI (do lado do host) e mandado ao daemon como no `vault add-ssh`. Uma chave com passphrase é recusada com a dica do `sandmux vault add-ssh`.
- **`VaultSetDomains`:** troca `domains` do segredo pelo nome, com a mesma validação do `VaultAdd`; para chave SSH, os servidores. Protocolo 24.
- **Erros do daemon:** aparecem na linha de aviso da página.

## Risks / Trade-offs

- **[Senha na memória da TUI]** Fica só enquanto o campo está aberto, em `Zeroizing`, e não vai para o desenho (só `•`).
