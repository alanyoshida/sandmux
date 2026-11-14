# Spec Delta

## ADDED Requirements

### Requirement: Desbloqueio do cofre na TUI
Quando o cofre existe e está trancado, a TUI MUST mostrar, ao abrir, um modal pedindo a senha, sem mostrar o que é digitado. Enter MUST desbloquear; Esc MUST fechar o modal e deixar o cofre trancado. Uma senha errada MUST mostrar o erro no modal e pedir de novo. As abas "aguardando cofre" MUST mostrar o estado na barra.

#### Scenario: Senha na abertura
- **WHEN** o cofre está trancado e o usuário abre a TUI, digita a senha certa e Enter
- **THEN** o cofre fica desbloqueado, e as sessões aguardando o cofre sobem
