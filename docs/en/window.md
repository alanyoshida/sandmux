# Desktop windows and audio

> Português: [window.md](../window.md)

By default, nothing from the desktop gets into the sandbox: no Wayland, X11, PipeWire or D-Bus socket, and the graphical session variables are removed. The `window` option lets the agent open windows on your desktop (a browser to test a page, the app it built) and use audio.

## Turning it on

```sh
sandmux run opencode --window          # restricted, through the security-context
sandmux run opencode --window-direct   # direct, through the plain socket
sandmux run opencode --no-window       # off, even with the configuration
sandmux config set --global window true
```

In the configuration, `window` takes `true`, `false` or `"direct"`, in the global section, profiles and projects, with the same precedence as the other on/off keys ([config.md](config.md)). In the TUI, the "Window" item of the Sandbox section, in Ctrl+b `e`, cycles through the three. The default is off.

- **Only in daemon tabs:** the window works in tabs opened in a terminal or from the TUI. Without a terminal (script, pipe), `run` with the window ends with an error, because the Wayland relay lives in the daemon.
- **Warning:** before launching, `run` warns that the agent can open windows, play sound, record the microphone and listen to other apps. In direct mode, the warning also says it can read the clipboard and, depending on the compositor, capture the screen.
- **Mark:** the TUI bar shows `▣` in the name of the tabs with the window on.

## Restricted mode (`true`)

1. sandmux registers a socket of the tab's own with the compositor, through the `wp_security_context_v1` protocol, with `sandmux` as the engine and app_id and the tab's identifier. This way the compositor knows the clients come from a sandbox.
2. In front of it, the daemon puts a relay of the Wayland protocol, and the relay's socket is the one that goes into the sandbox.
3. The relay shows the sandbox only the globals of an allowlist of common protocols: window, buffers (shm, dmabuf), focused input (keyboard, pointer, text-input), focus-based clipboard (normal copy and paste), scaling, color, decoration and the KWin effects.
4. The rest stays hidden, such as:
   - the background clipboard (`ext_data_control`, `zwlr_data_control`);
   - screen capture and the virtual keyboard and mouse;
   - input methods;
   - the list and control of other programs' windows;
   - the layers above the desktop (`zwlr_layer_shell`, `org_kde_plasma_shell`);
   - the screen lock and shortcut capture;
   - any protocol that is not on the list.
5. A program that tries to use a hidden global has its connection closed.

The relay exists because the compositor alone may not be enough: KWin 6.6 hides from security-context clients only the security-context protocol itself, and the background clipboard would stay within the agent's reach.

Without `wp_security_context_v1` (KDE 6 and sway have it; other compositors may not), or without a Wayland session, the launch is refused with the explanation. Direct mode is the explicit fallback.

## Direct mode (`"direct"`)

Your compositor's plain socket goes into the sandbox, without the relay. The agent sees every protocol the compositor offers a regular app: on KDE, including the background clipboard.

## Audio

With the window on, in either mode, your user's `pipewire-0` and `pulse/native` sockets also go in, when they exist. PipeWire does not separate playing from recording: the agent can play sound, record the microphone and listen to other apps. Separating them would require changing the system's PipeWire configuration, which sandmux does not do.

## Inside the sandbox

- `XDG_RUNTIME_DIR=/run/user/<uid>`, a private in-memory folder, with `wayland-0`, `pipewire-0` and `pulse/native`;
- `WAYLAND_DISPLAY=wayland-0` and `XDG_SESSION_TYPE=wayland`;
- hints for the toolkits to use Wayland: `GDK_BACKEND`, `QT_QPA_PLATFORM`, `MOZ_ENABLE_WAYLAND`, `ELECTRON_OZONE_PLATFORM_HINT`, `SDL_VIDEODRIVER`.

X11 never goes in: `DISPLAY` does not exist, and no X11 socket is mounted, because X11 allows capturing the whole desktop's keyboard. A program that only speaks X11 does not open. The GPU already goes in by default ([config.md](config.md)).

## Limits

- No D-Bus and no portals: an app that asks for a system dialog, the theme through the portal or a link opened in the host browser cannot get it. The portal with appearance, notifications and link opening filtered by the network policy is the next step.
- An app that needs a protocol outside the allowlist loses that feature, usually optional, or does not open.
- After a daemon restart, the tab rebuilds the socket. If the compositor is not up yet, it comes back ended with the message, and Ctrl+b `r` tries again.
