# Cloud credentials

> Português: [cloud.md](../cloud.md)

The agent uses `gcloud`, the Google Cloud libraries, `kubectl`, `aws` and the AWS SDKs inside the sandbox, but no real cloud credential enters it. Each cloud identity in the vault becomes a fake token in the tab. The real one stays outside the sandbox, with the daemon, and the gateway puts it in place of the fake only in requests to that identity's cloud. A fake copied out is worth nothing.

sandmux covers GCP, Kubernetes ([below](#kubernetes)) and AWS ([below](#aws)).

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

The Google libraries over gRPC (Pub/Sub, Firestore, Spanner) work too: the gateway speaks HTTP/2 with them and with Google, and swaps the token on each request (see [network.md](network.md#tls-decryption)).

The real one stays only in the daemon's memory, with its validity, and is renewed before it expires: the gateway always gets a real one with at least 5 minutes left, also on a reused connection. With the vault locked, nothing is swapped.

When the real one is missing (`gcloud` is not on the host, asks for a new login, or Google refuses the copy), the gateway lets the fake go on, and the tab's token request gets `invalid_grant` with what to do on the host. The daemon warns once per identity, through the desktop notification and in its log, until the real one comes back. `sandmux doctor` warns about each reference without `gcloud` on the host.

## What the agent can do

Whatever the identity's IAM allows: the gateway does not filter cloud requests, it only logs them. Pick an identity with the minimum, preferably a service account of its own for the agent. Two permissions let the agent get a real credential through the API itself, and should be left out:

- creating service account keys (`iam.serviceAccountKeys.create`);
- reading Secret Manager secrets.

Like the API keys, the identity applies to every tab, in any folder.

## Limits

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

- **kubeconfig `proxy-url`:** the gateway connects straight to the server.
- **`insecure-skip-tls-verify`:** when the host kubeconfig asks for it, the gateway does not verify the server either.
- **A renewed client certificate** takes effect on the next connection; a long connection (`get -w`) keeps the previous one until the server closes it.

## AWS

### Storing the profiles

With the vault unlocked, one command stores every profile of the host (the ones in `~/.aws/config` and `~/.aws/credentials`, or in `AWS_CONFIG_FILE` and `AWS_SHARED_CREDENTIALS_FILE`), each as an identity:

```sh
# Every profile, by reference to the host (the default)
sandmux vault add-cloud aws

# Only some profiles
sandmux vault add-cloud aws --profile dev --profile ci

# Copy of the long-term key of an IAM user
sandmux vault add-cloud aws --profile ci --copy
```

The name in the vault is `aws:<name>`, with the same rule as Kubernetes: `Dev_SSO` becomes `aws:dev-sso`. A name already in the vault is skipped, with a warning. The identity of the `default` profile is the default; without it, the one of the `AWS_PROFILE` profile, or the first one stored.

- **Reference to the host (the default):** the vault keeps only the profile, the region and the variables of the host files. The daemon gets the real one from the host `aws` (`aws configure export-credentials --profile <profile> --format process`), with your environment, outside the sandbox. SSO, `assume-role` and `credential_process` work as in your terminal; when SSO expires, run `aws sso login` on the host.
- **Copy (`--copy`):** both parts of the profile's long-term key are encrypted in the vault, and the gateway signs with it directly. A profile without a static key (SSO, `role_arn`, `credential_process`) or with a session token cannot be copied.

Without `--profile`, a refused profile is skipped with the reason. `sandmux vault list` shows each identity with the source, the region and the fake ID, never the key.

### In the tab

With at least one AWS identity, every tab gets, only during the run, a writable `~/.aws` folder (`AWS_CONFIG_FILE` and `AWS_SHARED_CREDENTIALS_FILE` point to it):

- one profile per identity, with the original name and the region;
- one fake pair per profile: the key ID (`AKIASANDMUX…`, 20 characters) and a fake secret key;
- the `[default]` with the default identity.

`aws` trusts the launch CA (`AWS_CA_BUNDLE`), and its cache (`~/.aws/cli/cache`) goes away at the end.

### In the gateway

With an AWS identity in the vault, every tab decrypts `*.amazonaws.com`. On AWS, the secret does not travel in the request: the SDK signs each request (SigV4) with the secret key. The gateway finds the identity by the fake ID in the request and redoes the signature with the real key, over the same canonical request:

- in the `Authorization` and in the presigned URL (`X-Amz-Signature`);
- with the real session token (`X-Amz-Security-Token`), when the real one is temporary;
- in S3 uploads with signed `aws-chunked` (`STREAMING-AWS4-HMAC-SHA256-PAYLOAD`, also with the trailer), chunk by chunk, streaming.

A request without `x-amz-content-sha256` has its body read for the hash, up to 32 MiB; above that, `413`. The log has the identity name (`key: aws:dev`), never the key.

**Credentials issued inside the tab:** the answer of STS (`AssumeRole`, `GetSessionToken`, `GetFederationToken` and the like), of SSO's `GetRoleCredentials` or of Cognito's `GetCredentialsForIdentity` carries a real credential. The gateway swaps it for new fakes (`ASIASANDMUX…`), and the daemon keeps the real one only in memory, until it expires. Requests with those fakes are re-signed with the stored real one (`key: aws:dev+sts` in the log). An answer that cannot be swapped (compressed, too large, or with the vault locked) is refused with `502`. A `sandmux daemon restart` forgets those credentials.

The real one is fetched again before it expires, with a 5-minute margin; without an expiry (a static key), every 10 minutes. When the real one is missing (`aws` is not on the host, SSO expired, the profile is gone), the gateway answers `403` with an error in the AWS format that says what to do on the host, and the daemon warns once. `sandmux doctor` warns about each reference without `aws` on the host.

### What the agent can do

Whatever the IAM of the profile allows: the gateway does not filter requests. Prefer a role of its own for the agent, with the minimum, and remove from the vault the production profiles it must not see. Two permissions hand out a real credential that the gateway does not swap, and should stay out:

- creating access keys (`iam:CreateAccessKey`): the new key comes back in the answer;
- reading secrets from Secrets Manager.

### AWS limits

- **SigV4a** (`AWS4-ECDSA-P256-SHA256`, from multi-region access points): the request passes unchanged and fails at AWS.
- **Event streams signed in the request** (Transcribe streaming): the gateway redoes the request signature, but not the signature of each event in the stream. Streamed answers (Bedrock `InvokeModelWithResponseStream`) work.
- **China** (`*.amazonaws.com.cn`) and the `*.api.aws` endpoints are not decrypted.
- **`aws configure` or `aws sso login` inside the tab** write to the tab's folder, which goes away at the end. Log in on the host.
