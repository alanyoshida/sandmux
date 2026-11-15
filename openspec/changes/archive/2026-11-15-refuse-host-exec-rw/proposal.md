## Why

O `--rw` (e a chave `rw`, a pasta de trabalho e as pastas montadas ao vivo) aceita arquivos e pastas que o host executa fora do sandbox, como `~/.bashrc`, `~/.profile`, `~/.local/bin` e `~/.config/autostart`. Com escrita neles, o agente pode plantar algo que roda fora do sandbox no próximo terminal ou login.

O achado é do change 3 e ficou como decisão em aberto no roadmap. Decisão do usuário (2026-11-15): recusar.

## What Changes

- **Recusa:** a montagem com escrita de um caminho que o host executa, de dentro dele ou de uma pasta que o contém, passa a ser recusada com o motivo e a dica de montar em somente leitura.
- **Onde vale:** em todos os pontos que já recusam caminhos perigosos:
  - `--rw` e a chave `rw`;
  - a pasta de trabalho;
  - `sandmux config add rw`;
  - as pastas montadas ao vivo (Ctrl+b `o`, `sandmux folder add --rw`).
- **Caminhos:**
  - arquivos de inicialização de shell e de sessão: `.bashrc`, `.bash_profile`, `.bash_login`, `.bash_logout`, `.profile`, `.zshrc`, `.zprofile`, `.zshenv`, `.zlogin`, `.zlogout`, `.xprofile`, `.xinitrc`, `.xsession`, `.xsessionrc` e `.pam_environment`;
  - pastas: `.bashrc.d`, `.oh-my-zsh`, `.local/bin`, `.local/share/applications`, `.config/autostart`, `.config/systemd`, `.config/environment.d`, `.config/fish` e `.config/plasma-workspace`.
- **Fica igual:** `--ro` nesses caminhos continua aceito.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `sandbox-filesystem`: a recusa da escrita em caminhos que o host executa.

## Impact

- **`sandmux-policy`:** `compose.rs` (`check_dangerous`), com testes.
- **Testes:** de integração do `run` e do `config add`.
- **Documentação:** `docs/config.md`, README e as versões em inglês.
