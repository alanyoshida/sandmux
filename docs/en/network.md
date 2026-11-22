# Network

> Português: [network.md](../network.md)

Each sandbox has its own network. No packet from the agent leaves without going through the sandmux gateway, which redoes each connection from the host, logs the destination and decides by the tab's policy: the network mode, the denylist and the allowlist.

## How it works

```text
sandbox                                   supervisor (sandmux-run, on the host)
+-------------------------------+         +----------------------------------+
| lo                            |         | gateway                          |
| sandmux0  10.0.2.100/24       |  TUN    |   TCP and UDP redone by the host |
|   default route 10.0.2.2 -----+-------->|   DNS at 10.0.2.3                |
|   DNS 10.0.2.3                |         |   10.0.2.254 -> host 127.0.0.1   |
+-------------------------------+         |   log in ~/.local/state/...      |
                                          +----------------------------------+
```

- **Its own network namespace, without privilege.** The sandbox's process 1 creates the `sandmux0` interface (a TUN) and hands the descriptor to the supervisor, the `sandmux-run` that stays on the host. The agent does not see the host's interfaces.
- **Gateway in the supervisor.** A user-space TCP/IP stack terminates each connection from the sandbox and opens another one from the host. The gateway lives exactly as long as the sandbox lives, and it also works in `sandmux run` without a terminal, without the daemon.
- **IPv4 only.** The sandbox has no IPv6; programs fall back to IPv4.
- **No ICMP.** `ping` does not work.

## Addresses

| Address | What it is |
|---|---|
| `10.0.2.100` | the sandbox |
| `10.0.2.2` | default route (the gateway) |
| `10.0.2.3` | name server |
| `10.0.2.254` | the machine: `host.sandmux.internal`, taken to the host's `127.0.0.1` |

The sandbox's `/etc/resolv.conf` points to `10.0.2.3`. The gateway answers `host.sandmux.internal` and forwards the other queries to the host's name servers (the `nameserver` entries in its `/etc/resolv.conf`, or `127.0.0.53`).

## Services on the machine

The sandbox's `127.0.0.1` is its own, not the host's. To reach a service running on your machine (a local database, a development server, a port published by the [private Docker](docker.md)), use `host.sandmux.internal`:

```bash
curl http://host.sandmux.internal:8080/
```

The `SANDMUX_HOST` variable holds that name. The local network (the other computers on your network) is reached normally.

### Host ports

A harness configured on the host for a local service, such as LM Studio at `http://127.0.0.1:8888/v1`, an Ollama or an MCP on `127.0.0.1`, gets `ECONNREFUSED` in the sandbox, because there `127.0.0.1` belongs to the sandbox itself. The `host-ports` key takes those host ports to the `127.0.0.1` inside, without editing the harness configuration:

```bash
sandmux config add host-ports 8888 --global
sandmux run opencode --host-port 9000
```

- **How it works:** the supervisor listens on `127.0.0.1:port` inside the sandbox's network namespace and forwards each connection to the host's `127.0.0.1:port`.
- **Log:** each connection produces a `tcp` line for `host.sandmux.internal` on `127.0.0.1:port`.
- **Policy:** the port is an explicit permission and applies in any mode with network, including `custom` and `inferencia`. Only a denylist rule matching `host.sandmux.internal` refuses it. In the no-network mode there is no gateway, and the key does not apply.
- **Live:** a port added while the session is alive takes effect within 2 seconds; one removed closes.
- **Port in use:** if the agent already listens on that port inside the sandbox, the forwarding does not open, and the log says why (once a minute).
- **Key in the vault:** a local server that asks for a key (Unsloth Studio, for example) gets the real value without it entering the sandbox. Store the key in the vault with the domain `host.sandmux.internal` (`sandmux vault add unsloth --domain host.sandmux.internal`, or through `sandmux vault import`, when the import replaced the key with a fake).
  - When the connection starts with an HTTP/1.x request, it goes through the same proxy as the decrypted connections, which swaps the fake for the real value in the headers and the path.
  - Each request produces an `http` line, with the method, the path, the status and the secret's name.
  - Without a secret for the host, or without HTTP, the connection is copied as is. A local server with TLS gets no swap.
- **Models in opencode:** opencode only shows, in a custom provider, the models written in the configuration. When launching an opencode session, the daemon asks the local servers which ones exist and registers them for that run only, without editing files. The list is rebuilt at each launch: Ctrl+b r in the tab picks up a new model from the server.
  - **Which providers:** those pointing to `127.0.0.1`, `localhost` or `[::1]` (the `baseURL`, or the default of `lmstudio`, port 1234, and of `ollama`, port 11434), with the port in `host-ports`.
  - **Query:** it is `GET <baseURL>/models`, with 1.5 s to connect and 6 s for the response (Unsloth Studio takes about 1.6 s). An `Authorization` or `apiKey` header with a fake value goes with the real one from the vault (domain `host.sandmux.internal`), outside the sandbox.
  - **Result:** the models go into `OPENCODE_CONFIG_CONTENT`, added to those from the configuration. A server that is down leaves the provider as it is.
  - **Locked vault:** without the vault, the query has no key, and the list comes back empty. That is why, after a daemon restart or a reboot, an opencode tab whose local providers use a key from the vault stays "waiting for vault" and starts, already with the list, on unlock.
- **No echo:** the host port is not published back as an [inbound port](#published-ports).
- **Import:** when the imported configuration of a harness mentions `127.0.0.1`, `localhost` or `[::1]` with a port, `sandmux import` lists the ports and the command for each one.
- **Chat tab:** gets no host ports; a local model server enters as a custom provider ([chat.md](chat.md#custom-provider)).

## Modes

| Mode | What goes through |
|---|---|
| `aberta` (default) | Everything, except the denylist |
| `inferencia` | Only the providers in the [inference catalog](#inference-provider-catalog) |
| `custom` | Only the allowlist |
| `sem-rede` | Nothing: only `lo`, and every outbound connection fails right away with network unreachable |

In every mode the denylist and the [fixed blocks](#fixed-blocks) apply. The mode comes from the `network` key of the [configuration](config.md), with the layer precedence, or from `--network` in `run`:

```bash
sandmux run shell --network sem-rede
sandmux config set network custom --project ~/projetos/api
```

## Policy

### Patterns

The items of `deny`, `allow`, `allow-off` and `catalog-off` are destination patterns:

| Pattern | Matches |
|---|---|
| `exemplo.com` | only `exemplo.com` (case-insensitive) |
| `*.exemplo.com` | any subdomain, such as `a.exemplo.com` and `a.b.exemplo.com`, but not `exemplo.com` |
| `192.168.1.10` | that IPv4 |
| `192.168.0.0/16` | the range, useful for the local network in `custom` mode |

A pattern outside these formats (with a scheme, a path or a space) makes the file or the option invalid.

### Denylist

The `deny` key and the `--deny` option add patterns to the denylist, combined across the layers. Denying always wins: a destination in the denylist is blocked in every mode, even if it is also in the allowlist.

The denylist already comes with the **exfiltration destination catalog**, on and versioned with the binary. sandmux never updates it over the network. Each item can be turned off with `catalog-off`:

- **text paste:** `pastebin.com`, `paste.ee`, `dpaste.com`, `dpaste.org`, `paste.rs`, `termbin.com`, `ix.io`, `sprunge.us`, `hastebin.com`, `rentry.co`, `controlc.com`
- **file upload:** `transfer.sh`, `0x0.st`, `file.io`, `tmpfiles.org`, `catbox.moe`, `litter.catbox.moe`, `gofile.io`, `bashupload.com`, `temp.sh`, `oshi.at`, `wetransfer.com`, `*.wetransfer.com`, `*.sendspace.com`
- **request catchers:** `webhook.site`, `*.webhook.site`, `requestbin.net`, `*.m.pipedream.net`, `*.free.beeceptor.com`, `requestcatcher.com`, `*.requestcatcher.com`
- **OAST and exfiltration DNS:** `*.oast.fun`, `*.oast.live`, `*.oast.pro`, `*.oast.site`, `*.oast.online`, `*.oast.me`, `*.oastify.com`, `*.burpcollaborator.net`, `*.dnslog.cn`, `*.ceye.io`, `*.interact.sh`
- **tunnels:** `*.ngrok.io`, `*.ngrok-free.app`, `*.ngrok.app`, `*.loca.lt`, `*.trycloudflare.com`, `serveo.net`, `*.serveo.net`, `*.localhost.run`, `*.pinggy.link`

```bash
sandmux config add catalog-off pastebin.com --global
```

### Allowlist

In `custom` mode only what is in the allowlist goes through: the patterns of `allow` and `--allow`, combined across the layers, and the groups of the common catalog that are on. A pattern in `allow-off`, in any layer, stays registered and turned off. Outside `custom` mode the allowlist has no effect.

The **common domain catalog** comes turned off. Each group is turned on with `catalog`:

| Group | Domains |
|---|---|
| `github` | `github.com`, `*.github.com`, `*.githubusercontent.com`, `ghcr.io` |
| `gitlab` | `gitlab.com`, `*.gitlab.com` |
| `npm` | `registry.npmjs.org`, `registry.yarnpkg.com` |
| `pypi` | `pypi.org`, `files.pythonhosted.org` |
| `crates` | `crates.io`, `index.crates.io`, `static.crates.io` |
| `go` | `proxy.golang.org`, `sum.golang.org` |
| `rubygems` | `rubygems.org`, `index.rubygems.org` |
| `maven` | `repo1.maven.org`, `repo.maven.apache.org` |
| `docker` | `registry-1.docker.io`, `auth.docker.io`, `production.cloudflare.docker.com` |
| `node` | `nodejs.org` |
| `rust` | `static.rust-lang.org` |
| `opencode` | `opencode.ai`, `models.opencode.ai` |

```bash
sandmux config set network custom --project ~/projetos/api
sandmux config add catalog npm --project ~/projetos/api
sandmux config add allow api.minha-empresa.com --project ~/projetos/api
```

### Inference provider catalog

The `inferencia` mode allows only: `openrouter.ai`, `api.anthropic.com`, `platform.claude.com` (Claude Code login renewal, [claude.md](claude.md)), `api.openai.com`, `generativelanguage.googleapis.com`, `oauth2.googleapis.com`, `aicode.googleapis.com`, `cloudcode-pa.googleapis.com` and `daily-cloudcode-pa.googleapis.com` (the Antigravity CLI Google login, [agy.md](agy.md#google-login)), `api.deepseek.com`, `api.mistral.ai`, `api.groq.com`, `api.x.ai`. Each provider can be turned off with `catalog-off`. When the secrets vault exists, the list comes from the providers registered in it.

### How the gateway decides

- **DNS:** a denied name, or one outside what the mode allows, gets NXDOMAIN without querying the host's resolver.
- **Name of a connection:** it comes from the queries the sandbox itself made to the gateway's DNS (an IP can be linked to several names, as in a CDN), from the TLS SNI and from the HTTP `Host` header.
- **`aberta` mode:** blocks when the SNI or the `Host` match the denylist, or, without them, when the most recent name of the IP matches, or when the IP matches a denied range.
- **Modes `custom` and `inferencia`:** lets it through only when a name linked to the IP (or the IP itself) is allowed and, if there is an SNI or `Host`, that one is too. That way, a forged SNI does not lead to an IP the sandbox did not query. A connection with no known name is blocked as "no domain".

- **SSH:** a TCP connection is SSH when the first bytes from the client, or from the server (for a client that waits for the server to speak), start with `SSH-`, on any port. After the mode's decision, it only goes on when a name linked to it matches a server of the [SSH keys](vault.md#ssh-keys) chosen for the session; otherwise it is closed with the reason `ssh fora dos servidores das chaves` ("ssh outside the key servers"). Without chosen keys (or with the vault locked), no SSH connection goes out.

The reason goes to the log (stored in Portuguese; `sandmux net` and the TUI show it in English): the denylist item (`denylist: x` or `catálogo de exfiltração: x`, "exfiltration catalog"), `fora da allowlist` ("outside the allowlist"), `fora do modo só inferência` ("outside inference-only mode") or `sem domínio` ("no domain").

### Immediate effect

In an alive session, a change takes effect on the next connection, without relaunching the harness:

- in the configuration file, through `sandmux config` or by hand (the daemon checks the file every second);
- in the tab's rules, through `sandmux net` or the TUI's network screen.

The daemon writes each launch's policy to `$XDG_RUNTIME_DIR/sandmux/net/<tag>.json`, and the gateway rereads it when the file changes. Switching between `aberta`, `inferencia` and `custom` takes effect right away; entering or leaving `sem-rede` takes effect at the next launch (`sandmux kill` and `sandmux restart`). `run` without a terminal, which has no daemon, uses the policy from the moment of the launch.

### Tab rules

The `--deny` and `--allow` options of `run` form the tab layer, the most specific one. It is stored in the session list and comes back on restart and restore. With the session alive:

```bash
sandmux net deny api.exemplo.com 1     # denies in session 1's tab
sandmux net allow registry.npmjs.org 1 # allows in the tab (custom mode)
sandmux net forget api.exemplo.com 1   # removes it from the tab's rules
sandmux net mode custom 1              # switches the tab's mode
sandmux net rules 1                    # mode and effective rules, with the origin
```

`sandmux net rules` without the session shows the rules for the current folder. Each line has the state (on or off), the pattern and the origin: a catalog, `global`, `profile <name>`, `project <folder>` or `tab`.

## TLS decryption

The gateway decrypts TLS only for the domains on the decryption list:

- the domains that own the secrets in the unlocked [vault](vault.md) (that is where, in the next stages, the fake key becomes the real one);
- the patterns of the `decrypt` key of the [configuration](config.md) and of the `--decrypt` option of `run`, combined across the layers.

The rest goes through encrypted end to end. The list changes right away, like the other rules: unlocking the vault or changing the configuration takes effect on the next connection.

- **Launch CA:** each launch generates its own CA. The key stays only in memory and in the specification's memfd, never on disk, and one sandbox does not trust another's CA.
- **Trust in the sandbox:** the system CA bundle (`/etc/ssl/certs/ca-certificates.crt` on Debian and Ubuntu) is covered by one with the system CAs plus the launch's. `SSL_CERT_FILE`, `NODE_EXTRA_CA_CERTS`, `REQUESTS_CA_BUNDLE`, `CURL_CA_BUNDLE`, `GIT_SSL_CAINFO`, `CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE` (`gcloud`) and `AWS_CA_BUNDLE` (`aws`) point to it.
- **In the gateway:** the sandbox's connection gets a certificate for the domain signed by the launch CA, with HTTP/1.1 only, and the gateway opens TLS to the real destination, verifying its certificate against the system CAs. An invalid certificate from the destination blocks the connection, with the reason `certificado inválido` ("invalid certificate") in the log.
- **Log:** each decrypted request becomes an `https` line with the method, the path without the query, and the status. Headers and body are never written. `sandmux net` and the TUI panel show `GET /v1/models 200`.
- **Certificate:** the domain certificate carries the Authority Key Identifier and the `serverAuth` usage, which Python 3.13+ requires in strict verification (`VERIFY_X509_STRICT`).
- **WebSocket:** an upgrade to WebSocket goes through without the `Sec-WebSocket-Extensions` header, so that messages do not come compressed, and messages flow in both directions until one end closes. In text messages from the sandbox to the owner of a secret, the fake becomes the real value ([hermes.md](hermes.md)). The connection becomes an `https` line with status `101`, written at the end, with the bytes in both directions.
- **Local server with HTTPS:** to decrypt a destination with its own CA (a development server with `mkcert`), point `SANDMUX_UPSTREAM_CA` to that CA's PEM when launching.

```bash
sandmux run shell --decrypt api.exemplo.com
sandmux config add decrypt api.exemplo.com --project ~/projetos/api
```

## Published ports

A TCP port that the agent starts listening on inside the sandbox, on `127.0.0.1`, `0.0.0.0` or `::`, is published on the host's `127.0.0.1` within 2 seconds: a development server on port 3000 opens in the browser at `http://127.0.0.1:3000`. When the number is in use on the host, the next free one above it is used. The host port closes when the agent stops listening.

- **List:** `sandmux stats <id>` and the Ports section of the TUI panel show each port, as `3000 → 127.0.0.1:3001 (sandbox)`.
- **How it works:** the supervisor reads the socket tables of the sandbox's namespace. A helper process, which entered the sandbox's user and network namespaces, opens each connection in there. The helper stays outside the sandbox's pid namespace, out of the agent's reach.
- **TCP only and, without an allowlist, only on the host's localhost.** Nothing is published on any other address of the machine.
- **Docker:** container ports go out through the folder's private Docker ([docker.md](docker.md)).

### Port allowlist

The `ports` key of the [configuration](config.md) (or `--port` in `sandmux run`) replaces publishing every listening port with an allowlist: only the listed ports leave the sandbox.

```toml
[global]
ports = ["8642", "8443@0.0.0.0"]
```

- **`8642`:** published on the host's `127.0.0.1`, as before. It does not accept connections through any other address of the machine.
- **`8443@0.0.0.0`:** also listens on the given address, on the same number, to receive webhooks directly (the hermes gateway, for example). With `0.0.0.0`, a single listener, on `0.0.0.0:8443`, also covers `127.0.0.1`. With a specific address (`8443@192.168.1.10`), there are two listeners, the one on `127.0.0.1` and the one on the address. `sandmux run` warns when a port is exposed to the network.
- **Number in use:** if the number is in use on the address, the port is not published on it, and the log gets an `inbound` line with the reason in `blocked`. On `127.0.0.1` the next free one is used, as without an allowlist.
- **Live:** the allowlist goes in the network policy, which the gateway rereads. A change in the configuration takes effect within 2 seconds, without relaunching the agent: the ports that left close, and the ones that came in open.
- **Log:** with the allowlist filled in, each accepted connection becomes an `inbound` line in the log, with the address of who connected, the sandbox port, the bytes in both directions and the duration.
- **List:** `stats` and the panel show the address, as `8443 → 0.0.0.0:8443 (sandbox)`.

A port on `0.0.0.0` accepts connections from any machine that can reach yours. Use a specific address when the local network is enough, and prefer an authenticated tunnel for the internet.

## Fixed blocks

Always refused, and logged with the reason:

- **Cloud metadata** (`169.254.169.254` and `fd00:ec2::254`): the connection closes right away.
- **QUIC** (UDP on port 443): browsers and HTTP/3 clients fall back to TCP. That way the domain shows up in the TLS SNI and enters the log.
- **ICMP.**

## Log

Each TCP connection, each UDP flow and each DNS query becomes a JSON line in `$XDG_STATE_HOME/sandmux/net/YYYY-MM-DD.jsonl` (usually `~/.local/state/sandmux/net/`), with folder 700 and file 600. The content of the connections is never written.

| Field | What it is |
|---|---|
| `ts` | local date and time, when the connection ended |
| `tag` | the launch: `s<session>-<n>` in a session, `run-<pid>` in a `run` without a terminal |
| `workdir` | work folder |
| `proto` | `tcp`, `udp`, `dns`, `icmp`, `https` (a decrypted request), `ssh` (an SSH connection) `ssh-sign` (a signature by the SSH agent; the `domain` is the key's fingerprint and the `secret`, its name) or `inbound` (an inbound connection on a [port in the allowlist](#port-allowlist); the `domain` and the `ip` are the address of who connected, and the `port`, the sandbox port) |
| `domain` | domain, or "sem domínio" ("no domain") |
| `ip`, `port` | destination |
| `up`, `down` | bytes sent and received |
| `ms` | duration |
| `blocked` | reason for the block, or `null` |
| `new` | the domain showed up for the first time in this work folder |
| `method`, `path`, `status` | only on `https` lines (decrypted requests): the method, the path without the query, and the response status |
| `secret` | only when the gateway swapped the fake key for the real one in the request: the name of the [vault](vault.md) secret, never the value; on `ssh-sign` lines, the name of the SSH key (`ssh signature: <name>` in `sandmux net` and in the panel) |
| `pid`, `exe` | only on the TCP and UDP connections of tabs, when known: the [process](#process-of-each-connection) that opened the connection (the pid in the host namespace) and the executable |
| `proc_via` | where the process came from: `kernel` or `proc` |

The domain comes from the DNS query the sandbox itself made (the IP stays linked to the name for 10 minutes), from the TLS SNI or from the HTTP `Host` header. A direct connection by IP stays as "no domain". The known domains of each folder are kept in `net/known/`.

### Process of each connection

The TCP and UDP lines of tabs say which process opened the connection, to tell the harness's inference call apart from a `curl` launched by a tool or from a package fetched by an `npm install`. The arguments don't go on the line: they are in the [execution log](files.md#executions), by pid.

- **By the kernel (`kernel`):** with the [root helper](helper.md#execution-and-connection-observation), a BPF program on the exit of `connect` records the pid, the source port and the destination of each TCP connection of the tab, and the daemon keeps the executable. It works even for a process that has already ended when the line is written.
- **By `/proc` (`proc`):** without the helper, and for UDP, the gateway finds the connection's socket (by the exact sock_diag query in the sandbox's network namespace, or by `/proc/<pid>/net/tcp`) and the process that owns it in the sandbox's process tree, like `ss -p`. The lookup happens at the start of the connection, before the gateway opens the outbound connection, while the process waits for the reply. It costs about 1 ms per connection. On the reference machine, 200 short connections to a local server took 0.5 s with the helper and 0.7 to 0.8 s without it, all with the process.

DNS queries, which the gateway itself answers, and the lines of the [private Docker](docker.md) have no process: in Docker, the bridge's NAT changes the source port before the gateway.

### `sandmux net`

```bash
sandmux net      # the last 50 lines from all sessions and `run`s
sandmux net 1    # only from session 1
```

Shows the time, the session (`run` for a `run` without a terminal), the protocol, the domain, the destination, the bytes on each side, the process (`curl 4321`) and "new" or the block. It reads today's and yesterday's lines.

In the [TUI](tui.md), the Network section of the panel shows the active tab's latest connections, and an inactive tab gets `•` on the bar when it accesses a new domain and `!` when it has a block. The network screen (Ctrl+b `a`) shows the tab's accesses and rules and lets you deny, allow, turn items on and off, and switch the mode.

### Retention

The daemon deletes files older than 30 days and, if the total goes over 500 MB, the oldest ones until it fits. It runs when the daemon starts and every hour. Both limits live in `[global]`:

```toml
[global]
net-log-days = 7
net-log-max = "1G"
```

## Performance

A user-space TCP/IP stack is slower than the kernel's, as in `slirp4netns`. On the reference machine, a 148 MB download through the gateway ran at 11.6 MB/s, the same speed as the host: the limit was the link, not the gateway.

## Limits

- **Shared CDN in `aberta` mode.** A denylist destination behind the same CDN as an allowed site is only caught by the SNI or the `Host`. Whoever needs a guarantee uses `custom` mode.
- **Connections already open** are not cut by a new rule; the rule applies to the following connections.
- **Decryption with HTTP/1.1 only:** on decrypted domains, the gateway offers only HTTP/1.1; programs that pin the server's certificate fail on those domains.
- **Long connections** only show up in the log when they end.
- **Process without the helper:** a process that closes the connection without waiting for a reply may end before the `/proc` lookup, and the line has no process.
- **No IPv6 and no ICMP** inside the sandbox.
