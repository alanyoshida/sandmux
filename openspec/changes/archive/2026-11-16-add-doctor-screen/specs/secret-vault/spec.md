## ADDED Requirements

### Requirement: Mover as chaves reais pela TUI
Na seção Diagnóstico, `f` MUST fazer o mesmo que `sandmux doctor --fix`: para cada chave, um campo de domínios editável, preenchido pela inferência ou pela URL mais próxima da chave no arquivo. Enter confirma, e Esc pula o arquivo. As chaves de um arquivo MUST ir ao cofre só depois de todas confirmadas. O login do `agy` MUST pedir confirmação. Com o cofre trancado, MUST pedir a senha antes.

#### Scenario: Domínio sugerido e editado
- **WHEN** a chave `corp` do `auth.json` do opencode no HOME sintético não tem domínio conhecido, a linha seguinte tem `"baseURL": "https://api.exemplo.com/v1"`, e o usuário, com o cofre trancado, aperta `f`, digita a senha e troca o campo por `llm.exemplo.com`
- **THEN** o campo veio com `api.exemplo.com`, o cofre ganha o segredo com o domínio `llm.exemplo.com`, o arquivo passa a ter o falso, e o valor real nunca aparece na tela

#### Scenario: Arquivo pulado
- **WHEN** o arquivo tem duas chaves, e o usuário confirma a primeira e aperta Esc na segunda
- **THEN** nenhuma das duas vai ao cofre, e o arquivo fica como estava
