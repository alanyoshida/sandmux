## ADDED Requirements

### Requirement: Escrita em caminhos que o host executa é recusada
A montagem com escrita (`--rw`, a chave `rw`, a pasta de trabalho, `config add rw` e as pastas montadas ao vivo) MUST ser recusada, com o motivo e a dica de usar `--ro`, quando o caminho é, está dentro ou contém um caminho que o host executa fora do sandbox:
- os arquivos de inicialização de shell e de sessão do HOME (`.bashrc`, `.bash_profile`, `.bash_login`, `.bash_logout`, `.profile`, `.zshrc`, `.zprofile`, `.zshenv`, `.zlogin`, `.zlogout`, `.xprofile`, `.xinitrc`, `.xsession`, `.xsessionrc`, `.pam_environment`);
- as pastas `.bashrc.d`, `.oh-my-zsh`, `.local/bin`, `.local/share/applications`, `.config/autostart`, `.config/systemd`, `.config/environment.d`, `.config/fish` e `.config/plasma-workspace`.

O mesmo caminho em somente leitura MUST continuar aceito.

#### Scenario: Escrita no .bashrc
- **WHEN** o usuário passa `--rw ~/.bashrc`
- **THEN** o `run` termina com erro, diz que o host executa esse arquivo fora do sandbox e sugere `--ro`

#### Scenario: Pasta que contém um caminho executado
- **WHEN** o usuário passa `--rw ~/.config/systemd/user`
- **THEN** o `run` recusa, porque a pasta fica dentro de `~/.config/systemd`

#### Scenario: Somente leitura
- **WHEN** o usuário passa `--ro ~/.local/bin`
- **THEN** o sandbox sobe com a pasta em somente leitura
