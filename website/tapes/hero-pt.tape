# Animação principal da página inicial, em português. O primeiro quadro já
# mostra o .env que não abre; o bloqueio e o acesso liberado acontecem ao vivo.
Output ../static/img/hero-pt.webm

Set Shell "bash"
Set Theme "GruvboxDark"
Set FontFamily "Fira Code"
Set FontSize 16
Set Width 1440
Set Height 860
Set Padding 24
Set TypingSpeed 45ms
Env SANDMUX_LANG "pt"

Source setup.tape

Hide
Type "sandmux" Enter
Sleep 2s
Type "ls -a" Enter
Sleep 500ms
Type "cat .env" Enter
Sleep 1s
Show
Sleep 1.2s
Type "curl -sS https://pastebin.com -o /dev/null" Enter
Sleep 2.5s
Screenshot "../static/img/hero-pt.png"
Type "curl -sS https://example.com -o /dev/null" Enter
Sleep 3s
