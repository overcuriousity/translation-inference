# translation-inference

A self-hosted translator in front of any OpenAI-compatible endpoint: llama.cpp, vLLM, LM Studio, LiteLLM or a hosted API. It translates text as it streams in, transcribes audio and video, translates subtitle files, reads translations aloud and interprets a conversation between two people.

- **Text**: streamed translation, source language detected as you type, any target language by name, a context hint, Markdown and side-by-side views, dictation, Read aloud, Save to BitVault.
- **Files**: `.srt` and `.vtt` subtitles, translated cue by cue, timing kept.
- **Talk**: two speakers, two languages; each turn is transcribed, translated and read aloud. It stops recording when the speaker pauses.
- **Long texts** are cut at paragraph and sentence boundaries to fit the model's context window, with the tail of the previous translation passed along for consistent terms.
- **Access**: open on a personal install; with an access key, anonymous visitors get a limited free tier and the key unlocks files, longer texts and a separate endpoint. Visitors can connect their own endpoint when the server allows it.
- **API**: everything the pages do, under `/api/v1`, documented at `/docs` and in `/api/v1/openapi.json`.

It stores nothing: no database, no accounts, no logs of texts. Sessions live in memory.

## Run

You need [Rust](https://www.rust-lang.org/tools/install) 1.97 or later and, for transcription, `ffmpeg` on `PATH`.

```sh
cp config.example.toml config.toml   # point [app.backend] at your endpoint
cargo run --release -- --config config.toml
```

Open `http://localhost:8080`. Every key can also be set from the environment, `TRANSLATION_INFERENCE__APP__BACKEND__URL=...` for `app.backend.url`; see `config.example.toml`.

For a server, install a release bundle with `install.sh`: a static binary, a hardened systemd unit and an nginx template. [AIRGAP.md](AIRGAP.md) covers installing without network, TLS and secrets.

## Access

| Tier | Who | Can use | Limit |
|---|---|---|---|
| Personal | everyone, when no `access_key` is set | everything | none |
| Free | anonymous visitors, when `access_key` is set | Text and Talk | `free_char_limit` |
| Unlocked | the access key, in Settings or as `Authorization: Bearer <key>` | everything, on `[app.gated]` if set | `gated_char_limit` |
| Own endpoint | visitors who connect theirs in Settings (`byok = true`) | everything, on their endpoint | none |

With an access key, the API answers only to the key or to a browser session started on a page.

## API

```sh
curl -sS http://localhost:8080/api/v1/translate \
  -H 'Content-Type: application/json' \
  -d '{"text": "Guten Morgen", "target": "en"}'
```

`/api/v1/translate/stream` sends the same translation as server-sent events; `/api/v1/transcribe`, `/api/v1/subtitles`, `/api/v1/speech` and `/api/v1/detect` cover the rest. The full reference is at `/docs`.

## BitVault

With `[app.bitvault]`, texts can be saved as unlisted pastes, and `/?from=<paste address>` opens a paste's text for translation. BitVault's `translate_url` setting adds a Translate link to its paste pages that does exactly that.

## Development

The app follows the [overcuriousity dev manifest](https://github.com/overcuriousity/overcuriousity-dev-manifest) v2026.11.1; see [AGENTS.md](AGENTS.md) for the layout, commands and the deliberate exceptions.

## Licence

MIT, see [LICENSE](LICENSE). The bundled fonts (SIL OFL 1.1) and Lucide icons (ISC) keep their own licences; see [THIRD-PARTY-LICENSES](THIRD-PARTY-LICENSES).
