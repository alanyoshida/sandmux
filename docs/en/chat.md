# Chat with the models

> Português: [chat.md](../chat.md)

The chat tab talks to a model whose key is in the [vault](vault.md), without opening a harness. It opens from the TUI modal (Ctrl+b `i`) or from `sandmux chat`. The [command modal](#command-modal) (Ctrl+b `g`) uses the same models to write a command into the tab.

## Opening a conversation

In the TUI, Ctrl+b `i` opens the chat modal:

- **message:** the first question;
- **model:** starts with `chat-model` from `[assistant]`. The ↑ ↓ arrows go through the models in `models`, → opens the [providers' list](#model-list), and any other name can be typed.

Tab switches fields and reaches the `⏎ Send` button; Enter, or a click on the button, sends, and Esc cancels. The answer opens in a new chat tab, named `chat:<model>`.

The chat and command dialogs follow the [theme](tui.md#color-theme):

- the page background and text;
- the title in an accent-colored block, like the bar's blocks;
- the labels in the secondary color, and the active field underlined in the accent;
- the two-tone button, with `⏎` in a dark block and the label in the accent. With the Tab focus, the `⏎` block also turns accent.

From the command line:

```sh
sandmux chat "what is an inode?"
sandmux chat --model openrouter/anthropic/claude-sonnet-5
```

- **With a terminal:** creates the chat tab in the daemon and attaches, like `sandmux run`. Ctrl+b d leaves, and the conversation goes on in a sandmux tab (the footer of the text bar reminds you).
- **Without a terminal (script, pipe):** needs the text. Prints only the answer on standard output and exits, without storing anything. A provider error exits with code 1.

## Inside the tab

The tab shows the model at the top, the conversation in the middle and, at the bottom, the text bar and the `[ Send ]` button.

| Key | What it does |
|---|---|
| Enter or click on `[ Send ]` | Sends the text in the bar |
| Ctrl+j or Alt+Enter | Breaks the line |
| Ctrl+u | Clears the bar |
| Ctrl+c | Cancels the answer in progress |
| PgUp / PgDn and the mouse wheel | Scroll the conversation |

- The answer appears bit by bit, as it arrives (streaming).
- `/modelo <provider>/<model>` changes the model for the following messages. `/modelo` alone opens the [providers' list](#model-list), with a filter, to pick from.
- A provider error appears in the conversation, with its status and message, and the conversation goes on.
- The text appears as it came, without rendering Markdown. The chat tab has no tools: the model does not read files or run commands.

## Models and providers

The model is written `<provider>/<model>`. The first slash separates the provider; the rest goes to the provider as the model name. All providers are used through the OpenAI-compatible API (`<url>/chat/completions`).

| Provider | Base URL | Key in the vault (domain) |
|---|---|---|
| `openrouter` | `https://openrouter.ai/api/v1` | `openrouter.ai` |
| `anthropic` | `https://api.anthropic.com/v1` | `api.anthropic.com` |
| `openai` | `https://api.openai.com/v1` | `api.openai.com` |
| `gemini` | `https://generativelanguage.googleapis.com/v1beta/openai` | `generativelanguage.googleapis.com` |
| `deepseek` | `https://api.deepseek.com/v1` | `api.deepseek.com` |
| `mistral` | `https://api.mistral.ai/v1` | `api.mistral.ai` |
| `groq` | `https://api.groq.com/openai/v1` | `api.groq.com` |
| `xai` | `https://api.x.ai/v1` | `api.x.ai` |

The key of a known provider must be in the vault, tied to its domain:

```sh
sandmux vault add openrouter --domain openrouter.ai
```

Without it, the modal and `sandmux chat` refuse and show that command. With the vault locked, the modal opens the unlock first, and `sandmux chat` in a terminal asks for the password.

### Model list

Each provider's models come from `GET <url>/models`, requested inside the chat tab sandbox, with the key swapped by the gateway as in the chat. The list has the known providers with a key in the vault and the custom providers. A provider that fails is left out, with the reason at the top of the list.

The list opens:
- on `/modelo` without an argument, in the chat tab;
- with → in the model field of the chat and command dialogs;
- with Enter on "Chat model" and "Command model" in the settings page (`e` edits the text by hand), and with `a` on "Suggested models".

Typing filters by all the words, case-insensitive. ↑ ↓ choose, Enter uses the marked one (with none, the typed text), Ctrl+r reloads and Esc cancels. In the TUI, the list is kept until it closes.

### Custom provider

`[assistant.provider.<name>]` registers another compatible endpoint, such as a local server:

```toml
[assistant]
chat-model = "openrouter/deepseek/deepseek-v4.1-flash-1017"
command-model = "openrouter/deepseek/deepseek-v4.1-flash-1017"
models = ["openrouter/anthropic/claude-sonnet-5", "local/llama3"]

[assistant.provider.local]
url = "https://host.sandmux.internal:8443/v1"
```

- **Network:** the URL's host is allowed for the chat tabs.
- **Key:** if the vault has a secret whose domain is the URL's host, the key is swapped in the traffic. Without a secret, the request goes without `Authorization`.
- **HTTP:** the swap only happens over HTTPS. A local server over `http://` works, but without a key.
- **Names:** the name of a custom provider cannot repeat that of a known one.

## Confinement

The chat tab is a daemon session that runs the `sandmux-chat` program inside the sandbox, like the harnesses. The binary sits next to `sandmux-run` and enters read-only at `/run/sandmux/bin/sandmux-chat`.

- **Network:** always in inference-only mode, plus the host of each custom provider, whatever the `network` of the layers. The configuration's blocking rules still apply, and each request goes to the [network log](network.md).
- **Key:** the program receives only the vault's fake value (`OPENROUTER_API_KEY` and the other provider variables, or `SANDMUX_CHAT_KEY_<NAME>` for a custom provider). The gateway swaps it for the real one in the traffic, as with the harnesses. The providers reach the program in `SANDMUX_CHAT_PROVIDERS`.
- **No extras:** no private Docker, SSH agent, database MCP, repository, extra folders or `pass-env`.
- **Folder:** the work folder is a private folder of the tab, at `$XDG_STATE_HOME/sandmux/chat/<id>` (mode 700).

## The stored conversation

The conversation stays in `conversa.json`, in the tab's folder, with the model and the messages.

- **When it is written:** on each message sent and on each complete answer, through a temporary file that is renamed.
- **Interrupted answer:** is not written. On return, the last question has no answer, and the program sends it again.
- **Daemon restart and reboot:** the tab comes back with the conversation. With the vault created and locked, it shows "waiting for vault" and starts on unlock, like the [hermes gateway](hermes.md).
- **When the tab is closed** (Ctrl+b `x` or `sandmux kill`), the folder and the conversation are deleted.
- **Session list:** the conversation does not go into the daemon's list. Neither does the first message: it goes straight into `conversa.json`.

This is an exception to the screen history rule, which never goes to disk, and it applies only to the chat tab. It was decided by the user on 2026-11-14.

## Command modal

Ctrl+b `g` opens the command modal over the active tab: a question like "compress the logs folder without the .tmp files" becomes a command written on the tab's line.

1. **Question and model:** the model starts with `command-model` from `[assistant]` or, without it, with `chat-model`. Tab and the arrows work as in the chat modal.
2. **Enter, or a click on the `⏎ Send` button, asks.** Only the question, the tab's work folder and the shell name go to the model (the user's `SHELL` when it is in the system folders; otherwise, `bash`), with a system prompt that asks for a single command, without explanation. Nothing from the tab's screen is sent, unless the **screen** field is on (see below). The TUI keeps responding while the modal shows "waiting for the model".
3. **The suggested command** appears in an editable field, without the code fences and without a leading `$ `. Tab goes back to the question.
4. **Enter, or a click on the `⏎ Insert` button,** writes the command into the tab's input, without Enter: it only runs when the user presses Enter in the tab.

**Screen field:** with Tab to it and Space, it turns on sending the tab's last 40 visible lines, without the scrolled history and without the trailing empty lines, along with the question. It is for asking for the command that fixes an error that just showed up. The field starts off every time the modal opens, and, when on, the modal warns that the screen goes to the provider.

A multi-line command goes as a paste (between `ESC[200~` and `ESC[201~`) when the tab has turned on paste mode, as bash and zsh do. Without that mode, the modal refuses, because the line break would run the command.

The question goes through the daemon like that of `sandmux chat` without a terminal, with the system prompt in the `system` field of `ask` (protocol 21). It runs in the same confinement as the chat tab, with the key swapped at the gateway.

The folder sent is the tab's work folder, not the shell's current folder, which the TUI does not see.

## Question without a tab

Without a terminal, `sandmux chat` asks the daemon for the protocol's `ask` message (since protocol 20; the system prompt, since 21). The daemon then:

1. creates a temporary chat folder with the question;
2. runs `sandmux-chat --once` in the same confinement as the tab, with the output in a pipe;
3. returns the answer and deletes the folder.

The request goes through the daemon because only there can the gateway reach the vault to swap the key.
