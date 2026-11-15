## 1. Configuração e composição

- [x] 1.1 Chave `window` (`true`, `false`, `"direct"`) nas camadas, no efetivo com origem e no `config set`; opções `--window`, `--no-window` e `--window-direct` no `run`
- [x] 1.2 Composição: `XDG_RUNTIME_DIR` em memória, socket do compositor em `wayland-0`, `pipewire-0` e `pulse/native`, `WAYLAND_DISPLAY` e as dicas dos toolkits; nada sem a opção

## 2. Socket do compositor

- [x] 2.1 Módulo `window`: conexão ao compositor pelo `WAYLAND_DISPLAY` do cliente, `wp_security_context_v1` com motor, app_id e identificador da aba, `WindowSocket` que fecha e apaga ao soltar; erros com explicação
- [x] 2.3 Repassador do Wayland com a allowlist dos globais, `bind` escondido derruba a conexão, descritores repassados; `run` sem terminal recusa a janela
- [x] 2.2 Daemon: socket por sessão, guardado enquanto vive, refeito na restauração (modo e `WAYLAND_DISPLAY` na lista); `run` sem terminal segura o socket; protocolo 34 com o modo na sessão

## 3. Visibilidade

- [x] 3.1 Aviso antes do lançamento (restrito e direto)
- [x] 3.2 Marca `▣` na barra da TUI e o item `window` na tela de configurações

## 4. Testes e documentação

- [x] 4.1 Testes: configuração e precedência, composição por modo, compositor falso com e sem o protocolo, aviso, marca na TUI
- [x] 4.2 Verificação manual no KDE: janela aberta pelo socket da aba e `ext_data_control_manager_v1` fora do registro visto de dentro
- [x] 4.3 Documentação: `docs/window.md` novo, `docs/config.md`, README e os pares em inglês
