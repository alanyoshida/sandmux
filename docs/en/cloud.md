# Cloud credentials

> Português: [cloud.md](../cloud.md)

The agent uses `gcloud` and the Google Cloud libraries inside the sandbox, but no real cloud credential enters it. Each cloud identity in the vault becomes a fake token in the tab. The real one stays outside the sandbox, with the daemon, and the gateway puts it in place of the fake only in requests to the Google APIs. A fake copied out is worth nothing.

Today sandmux covers GCP. Kubernetes and AWS come later.

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
