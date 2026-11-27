# Telas paradas da galeria, em inglês.
Output "/tmp/sandmux-demo/gallery-en.webm"

Set Shell "bash"
Set Theme "GruvboxDark"
Set FontFamily "Fira Code"
Set FontSize 16
Set Width 1440
Set Height 860
Set Padding 24
Env SANDMUX_LANG "en"

Source setup.tape

Hide
Type "sandmux" Enter
Sleep 2s
Type "curl -sS https://example.com -o /dev/null; curl -sS https://pastebin.com -o /dev/null; curl -sS https://openrouter.ai/api/v1/models -o /dev/null; clear" Enter
Sleep 4s
Ctrl+b
Type "a"
Sleep 1.5s
Show
Sleep 500ms
Screenshot "../static/img/rede-en.png"
Hide
Escape
Sleep 500ms
Ctrl+b
Type "e"
Sleep 1.5s
Down 4
Sleep 800ms
Show
Sleep 500ms
Screenshot "../static/img/config-en.png"
Hide
Down 5
Sleep 500ms
Tab
Sleep 500ms
Type "g"
Sleep 1.5s
Down 1
Sleep 1s
Show
Sleep 500ms
Screenshot "../static/img/cofre-en.png"
