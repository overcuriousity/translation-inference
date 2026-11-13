# Airgapped install

Every release can be installed on a machine with no network access. translation-inference then talks only to the model endpoints you configure on that network, such as llama.cpp, vLLM or a local Whisper server.

## What to carry across

Download these from the release page on a connected machine:

| File | Purpose |
|---|---|
| `translation-inference-<version>-x86_64-unknown-linux-musl-offline.tar.gz` (or `aarch64`) | static binary, `install.sh`, systemd unit, nginx template, example config, licences, `SHA256SUMS` |
| `install.sh` | the installer, also inside each bundle |
| `SHA256SUMS` | checksums of the release files |
| `translation-inference-<version>-src-vendored.tar.gz` | optional: source with every dependency vendored, to build without network |

Transcription also needs `ffmpeg` on the machine; carry your distribution's package across if it is not installed.

Check the downloads before you carry them across:

```sh
sha256sum -c SHA256SUMS --ignore-missing
```

The release also carries build-provenance attestations. On a connected machine, check them with `gh attestation verify <file> --repo overcuriousity/translation-inference`.

## Install

The installer never uses the network and never calls `sudo`.

For your user only, into `~/.local/bin`:

```sh
sh install.sh --archive translation-inference-<version>-x86_64-unknown-linux-musl-offline.tar.gz
```

As a system service, run as root. This installs to `/usr/local/bin`, sets up a hardened systemd unit and creates `/etc/translation-inference/config.toml`:

```sh
sh install.sh --archive translation-inference-<version>-x86_64-unknown-linux-musl-offline.tar.gz --system
```

The installer checks every file against the bundle's own `SHA256SUMS` and stops with "Checksum mismatch" if one is damaged. That list travels inside the bundle, so it cannot show tampering. For that, pass the bundle's checksum from the release's `SHA256SUMS`, which you checked above:

```sh
sh install.sh --archive translation-inference-<version>-x86_64-unknown-linux-musl-offline.tar.gz --sha256 <checksum from SHA256SUMS>
```

## TLS on an airgapped network

translation-inference never terminates TLS itself. It listens on `127.0.0.1:8080`, and nginx on the same machine serves https.

Use either of these:
- A certificate from your local CA. This is best: browsers trust it once the CA is installed on each client.
- A self-signed certificate. Browsers will warn until each client trusts it.

```sh
openssl req -x509 -newkey rsa:3072 -sha256 -days 825 -nodes \
  -keyout /etc/ssl/private/translate.lan.key -out /etc/ssl/certs/translate.lan.pem \
  -subj "/CN=translate.lan" -addext "subjectAltName=DNS:translate.lan"
```

Then:
1. Copy `nginx.conf` from the bundle (also in `~/.local/share/translation-inference/` or `/usr/local/share/translation-inference/`) to `/etc/nginx/conf.d/translation-inference.conf`. Replace `translate.example.org` and the certificate paths, then reload nginx.
2. Set the address users type in `/etc/translation-inference/config.toml`:

   ```toml
   public_url = "https://translate.lan"
   ```

The app refuses to start if `public_url` is not https, except for `localhost`.

Microphones work only on https or localhost: browsers keep recording to secure contexts. Dictation and Talk need https.

### Plain http

On a trusted network you may skip TLS with `allow_plain_http = true` and `public_url = "http://translate.lan:8080"` (bind to a LAN address then). The session cookie loses `Secure`, the access key and every text cross the network in clear, and browsers will not record from the microphone. The app logs a warning at start-up.

## Secrets

Keep secrets out of `config.toml`; it is world-readable so the service's dynamic user can read it. Put a secret in a root-only file and hand it to the service as a credential:

```sh
printf '%s' 'the-access-key' | install -m 0600 /dev/stdin /etc/translation-inference/access-key
systemctl edit translation-inference   # add the two lines below under [Service]
# LoadCredential=access-key:/etc/translation-inference/access-key
# Environment=TRANSLATION_INFERENCE__APP__ACCESS_KEY_FILE=%d/access-key
```

API keys work the same way, for example `TRANSLATION_INFERENCE__APP__BACKEND__API_KEY_FILE`.

## Build from source without network

```sh
tar -xzf translation-inference-<version>-src-vendored.tar.gz
cd translation-inference-<version>
cargo build --release --offline --locked
```

The archive's `.cargo/config.toml` points Cargo at the bundled `vendor/` directory. Fonts and icons are already inside the source tree.

## First start

```sh
systemctl enable --now translation-inference
curl -s http://127.0.0.1:8080/api/v1/health
```

The health check answers `{"status":"ok", ...}`. The log (`journalctl -u translation-inference`) names the models found on each endpoint and warns when ffmpeg is missing.
