# Cloud credentials

> Português: [cloud.md](../cloud.md)

The agent uses `gcloud`, the Google Cloud libraries and `kubectl` inside the sandbox, but no real cloud credential enters it. Each cloud identity in the vault becomes a fake token in the tab. The real one stays outside the sandbox, with the daemon, and the gateway puts it in place of the fake only in requests to that identity's cloud. A fake copied out is worth nothing.

Today sandmux covers GCP and Kubernetes ([below](#kubernetes)). AWS comes later.

## Storing an identity

With the vault unlocked:

```sh
# Reference to a configuration of the host gcloud (the default)
sandmux vault add-cloud gcp work --configuration work

# Copy of the host ADC (the one from `gcloud auth application-default login`)
sandmux vault add-cloud gcp personal --copy-adc

# Copy of a service account JSON key
sandmux vault add-cloud gcp deploy --copy-key ~/Downloads/deploy.json --default
```

The name in the vault is `gcp:<name>`. Each identity has a source:

- **Reference to the host (`--configuration`):** the vault stores only the name of the `gcloud` configuration, the account and the project, never a token. For each request, the daemon gets the access token from the host `gcloud` (`gcloud config config-helper`), with your HOME and your environment, outside the sandbox. SSO, reauthentication and service accounts work as in your terminal: when `gcloud` asks for a new login, run `gcloud auth login` on the host.
- **Copy (`--copy-adc`, `--copy-key`):** the ADC refresh token or the service account key stays encrypted in the vault. The daemon mints the access token at Google's token endpoint. It serves to take the long-lived credential off the host disk: after the copy, you can delete the file.

The first GCP identity is the default, and `--default` makes another one the default. `sandmux vault list` shows each identity with the `cloud` kind, the source, the account, the project and the fake, never the token or the key. To change an identity, remove it with `sandmux vault remove gcp:<name>` and store it again.

## In the tab

With the vault unlocked and at least one GCP identity, every tab gets, only during the run:

- one `gcloud` configuration per identity (`~/.config/gcloud/configurations/config_<name>`), with the account, the project and the fake token in a file (`auth/access_token_file`);
- the default identity as the active one;
- the ADC (`~/.config/gcloud/application_default_credentials.json`) with a fake refresh token of the default, for the Google libraries (`google.auth.default()` and the like).

The agent switches identity as on the host: `gcloud config configurations activate deploy`, or `--configuration deploy` on a command. The tab's `gcloud` folder belongs to this run only: what the tab writes in it goes away at the end, and the folder stored in the synthetic HOME does not change. `gcloud` trusts the launch CA (`CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE`).

## In the gateway

With a GCP identity in the vault, every tab decrypts `*.googleapis.com`, and the gateway:

- swaps an identity's fake token for its current real one in the `Authorization` header;
- answers by itself the `POST /token` to `oauth2.googleapis.com` that carries the fake refresh token, with the fake access token, without going to Google;
- logs each request with the identity's name (`key: gcp:work` in `sandmux net`), never the token.

The real one stays only in the daemon's memory, with its validity, and is renewed before it expires: the gateway always gets a real one with at least 5 minutes left, also on a reused connection. With the vault locked, nothing is swapped.

When the real one is missing (`gcloud` is not on the host, asks for a new login, or Google refuses the copy), the gateway lets the fake go on, and the tab's token request gets `invalid_grant` with what to do on the host. The daemon warns once per identity, through the desktop notification and in its log, until the real one comes back. `sandmux doctor` warns about each reference without `gcloud` on the host.

## What the agent can do

Whatever the identity's IAM allows: the gateway does not filter cloud requests, it only logs them. Pick an identity with the minimum, preferably a service account of its own for the agent. Two permissions let the agent get a real credential through the API itself, and should be left out:

- creating service account keys (`iam.serviceAccountKeys.create`);
- reading Secret Manager secrets.

Like the API keys, the identity applies to every tab, in any folder.

## Limits

- **gRPC:** the gateway decrypts only HTTP/1.1. The Google libraries that use gRPC (Pub/Sub, Firestore, Spanner) fail with `*.googleapis.com` decrypted. `gcloud` and the REST APIs (Storage, BigQuery, Compute) work.
- **Old `gsutil` and `bq`:** they have their own authentication. Use `gcloud storage` and `bq` through `gcloud`.
- **Identity tokens:** `gcloud auth print-identity-token` and authenticated Cloud Run do not work: the fake is an access token.
- **`gcloud auth login` inside the tab:** it writes a real token into the synthetic HOME, as today. Log in on the host.

## Kubernetes

### Storing the contexts

With the vault unlocked, one command stores every context of the host kubeconfig (the one in `KUBECONFIG`, or `~/.kube/config`), each as an identity:

```sh
# Every context, by reference to the host (the default)
sandmux vault add-cloud kubernetes

# Only some contexts
sandmux vault add-cloud kubernetes --context kind-dev --context gke_app_europe-west1_main

# Copy of the server, the CA and the credential of a context
sandmux vault add-cloud kubernetes --context kind-dev --copy
```

The name in the vault is `kube:<name>`, with the context name in lowercase and each character other than letters, digits and `-` replaced by `-`: `arn:aws:eks:us-east-1:123:cluster/prod` becomes `kube:arn-aws-eks-us-east-1-123-cluster-prod`. A name already in the vault is skipped, with a warning. The identity of the host's current context is the default.

- **Reference to the host (the default):** the vault keeps only the context, the kubeconfig path, the server and the namespace. The daemon reads the context with the host `kubectl` (`kubectl config view --raw --flatten --minify`) and, when its user has an `exec` plugin (`aws eks get-token`, `gke-gcloud-auth-plugin`, `kubelogin`), runs the plugin on the host, without a terminal, with your environment. When the plugin asks for a new login, log in on the host (`aws sso login`, `gcloud auth login`).
- **Copy (`--copy`):** the server, the cluster CA and the credential (the token, or the client certificate with its key) are encrypted in the vault. A context with an `exec` plugin cannot be copied: the plugin only runs on the host.

sandmux refuses the old `auth-provider` and users with a password. Without `--context`, a refused context is skipped with the reason, and the others are stored. `sandmux vault list` shows each identity with the source, the server and the fake, never the token, the certificate or the key. To take a context away from the agent, `sandmux vault remove kube:<name>`.

### In the tab

With at least one Kubernetes identity, every tab gets, only during the run, a writable `~/.kube` folder and a kubeconfig with fakes only (`KUBECONFIG` points to it):

- one context per identity, with the original name and the host namespace;
- the server `https://<name>.kube.sandmux.internal`, with the launch CA (`~/.kube/sandmux-ca.crt`);
- one fake token per identity (`sandmux-fake-kube-…`);
- the current context on the default identity.

`kubectl config use-context`, the discovery cache and everything else `kubectl` writes stay in the tab's folder, which goes away at the end.

### In the gateway

The tab's DNS answers every `*.kube.sandmux.internal` with `10.0.2.253`, and that name passes the network policy without a rule, in any mode: storing the identity is the consent. The gateway decrypts the connection and connects by itself to the stored server, never to the destination the sandbox asked for, including a cluster on the host's `127.0.0.1` (kind and minikube). The server certificate is verified against the cluster CA, and the gateway presents the client certificate when there is one.

On each request, the gateway swaps the fake token for the real one (from the kubeconfig, the `tokenFile` or the plugin). When the credential is only the client certificate, the gateway removes the `Authorization`. The `Host` and the path prefix (Rancher's, for example) come from the real server. The log has the identity name and the real server, never the credential. `kubectl exec`, `attach`, `cp`, `logs -f`, `port-forward` and `get -w` work, over WebSocket or SPDY.

The real one is fetched again before the plugin's expiry. Without an expiry (a token or a certificate from the kubeconfig), it is reread every 10 minutes, and a change in the host kubeconfig takes effect within that time. When the real one is missing (`kubectl` is not on the host, the context is gone, the plugin fails), the gateway answers `401` with a Kubernetes `Status` that gives the reason, and the daemon warns once, as with GCP. A name without an identity gets `404`. `sandmux doctor` warns about each reference without `kubectl` on the host.

### What the agent can do

Whatever the RBAC of each context allows: the gateway does not filter requests to the API server. An administrator context makes the agent an administrator. Prefer a context with its own ServiceAccount and the minimum permissions, and remove from the vault the production contexts the agent must not see. Reading `secrets` gives the agent the real credentials the cluster stores.

### Kubernetes limits

- **HTTP/2:** the gateway offers only HTTP/1.1, and `client-go` switches to it by itself.
- **kubeconfig `proxy-url`:** the gateway connects straight to the server.
- **`insecure-skip-tls-verify`:** when the host kubeconfig asks for it, the gateway does not verify the server either.
- **A renewed client certificate** takes effect on the next connection; a long connection (`get -w`) keeps the previous one until the server closes it.
