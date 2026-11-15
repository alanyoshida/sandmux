## Why

O usuário pediu (2026-11-14) o sandmux todo em inglês, inclusive a documentação. O change 36 traduz o programa; este traz a documentação.

## What Changes

- **README:** `README.en.md`, a versão em inglês do README, com um link para ela no topo do README em português, e vice-versa.
- **Docs:** `docs/en/`, a versão em inglês de cada arquivo de `docs/`, com os mesmos nomes e os links entre si apontando para a versão em inglês.
- **Fonte de verdade:** o português continua a principal, como manda o AGENTS.md; um change que muda um doc em português atualiza também o inglês.
- **Teste:** cada doc em português tem o par em inglês, e os exemplos de configuração em inglês são válidos.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `interface-language`: a documentação em inglês.

## Impact

- **Arquivos novos:** `README.en.md` e `docs/en/*.md`.
- **Testes:** um teste em `crates/sandmux/tests/docs.rs`.
- **AGENTS.md:** a regra de manter os dois idiomas da documentação.
