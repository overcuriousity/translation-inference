# Airgapped install

Every release can be installed on a machine with no network access. Replace `app` with the binary name.

## What to carry across

Download these from the release page on a connected machine:

| File | Purpose |
|---|---|
| `app-<version>-x86_64-unknown-linux-musl-offline.tar.gz` (or `aarch64`) | static binary, `install.sh`, systemd unit, nginx template, example config, licences, `SHA256SUMS` |
| `install.sh` | the installer, also inside each bundle |
| `SHA256SUMS` | checksums of the release files |
| `app-<version>-src-vendored.tar.gz` | optional: source with every dependency vendored, to build without network |

Check the downloads before you carry them across:

```sh
sha256sum -c SHA256SUMS --ignore-missing
```

The release also carries build-provenance attestations. On a connected machine, check them with `gh attestation verify <file> --repo overcuriousity/overcuriousity-dev-manifest`.

## Install

The installer never uses the network and never calls `sudo`.

For your user only, into `~/.local/bin`:

```sh
sh install.sh --archive app-<version>-x86_64-unknown-linux-musl-offline.tar.gz
```

As a system service, run as root. This installs to `/usr/local/bin`, sets up a hardened systemd unit and creates `/etc/app/config.toml`:

```sh
sh install.sh --archive app-<version>-x86_64-unknown-linux-musl-offline.tar.gz --system
```

The installer checks every file against the bundle's `SHA256SUMS`. If any file has changed, it stops with "Checksum mismatch".

## TLS on an airgapped network

The app never terminates TLS itself. It listens on `127.0.0.1:8080`, and nginx on the same machine serves https. Plain http over the network is not supported, because secure cookies and passkeys require https.

Use either of these:
- A certificate from your local CA. This is best: browsers trust it once the CA is installed on each client.
- A self-signed certificate. Browsers will warn until each client trusts it.

```sh
openssl req -x509 -newkey rsa:3072 -sha256 -days 825 -nodes \
  -keyout /etc/ssl/private/app.lan.key -out /etc/ssl/certs/app.lan.pem \
  -subj "/CN=app.lan" -addext "subjectAltName=DNS:app.lan"
```

Keep secrets out of `config.toml`; it is world-readable so the service's dynamic user can read it. Put a secret in a root-only file and hand it to the service as a credential:

```sh
printf '%s' 'the-client-secret' | install -m 0600 /dev/stdin /etc/app/oidc-secret
systemctl edit app   # add: [Service] LoadCredential=oidc-secret:/etc/app/oidc-secret
```

and in `config.toml`: `client_secret = "file:///run/credentials/app.service/oidc-secret"`.

Then:
1. Copy `nginx.conf` from the bundle (also in `~/.local/share/app/` or `/usr/local/share/app/`) to `/etc/nginx/conf.d/app.conf`. Replace `journal.example.org` and the certificate paths, then reload nginx.
2. Set the address users type in `/etc/app/config.toml`:

   ```toml
   public_url = "https://app.lan"
   ```

The app refuses to start if `public_url` is not https, except for `localhost`.

## Build from source without network

```sh
tar -xzf app-<version>-src-vendored.tar.gz
cd app-<version>
SQLX_OFFLINE=true cargo build --release --offline --locked -p app
```

The archive's `.cargo/config.toml` points Cargo at the bundled `vendor/` directory. Fonts, icons and htmx are already inside the source tree.

## First start

On first start the app creates the user `app` and prints its password once to the log (`journalctl -u app`).

For a user install, add more accounts with `app user add <name> [--admin]`. For the system service, run the command inside the service's sandbox, so it uses the service's data directory and user:

```sh
systemd-run --pty --wait -p DynamicUser=yes -p StateDirectory=app \
  /usr/local/bin/app --config /etc/app/config.toml user add <name> [--admin]
```
