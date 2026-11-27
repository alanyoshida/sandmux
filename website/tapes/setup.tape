# Preparação comum, escondida: projeto de demonstração, configuração e duas
# sessões. Roda dentro do ambiente isolado de scripts/site-screens.sh.
Hide
Type "export PS1='$ ' && cd $SANDMUX_DEMO && mkdir -p $XDG_CONFIG_HOME/sandmux app/src api && clear" Enter
Type `printf '[global]\ndeny = ["*.ngrok.io"]\n\n[tui]\ntheme = "gruvbox"\n' > $XDG_CONFIG_HOME/sandmux/config.toml` Enter
Type `printf 'OPENROUTER_API_KEY=demo-nao-e-uma-chave\n' > app/.env && printf 'fn main() {}\n' > app/src/main.rs && printf '# app\n' > app/README.md && clear` Enter
Type `printf 'demo-senha\ndemo-senha\n' | sandmux vault init && printf 'demo-senha\n' | sandmux vault unlock && printf 'demo-nao-e-uma-chave\n' | sandmux vault add openrouter --domain openrouter.ai && printf 'demo-token-falso\n' | sandmux vault add github --domain github.com --domain api.github.com && printf 'demo-token-ci\n' | sandmux vault add github-ci --domain github.com && printf 'demo-token-npm\n' | sandmux vault add npm --domain registry.npmjs.org; clear` Enter
Sleep 3s
Type "cd app && sandmux run shell --no-import" Enter
Sleep 3s
Type "PS1='\W$ '; clear" Enter
Sleep 300ms
Ctrl+b
Type "d"
Sleep 1s
Type "cd ../api && sandmux run shell --no-import" Enter
Sleep 3s
Type "PS1='\W$ '; clear" Enter
Sleep 300ms
Ctrl+b
Type "d"
Sleep 1s
Type "cd ../app && clear" Enter
Show
