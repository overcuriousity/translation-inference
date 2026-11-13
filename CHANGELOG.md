# Changelog

All notable changes are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions are CalVer `YYYY.M.N`.

## [2026.11.0]

This is a rewrite to the overcuriousity dev manifest v2026.11.1 and a fresh start: the configuration, the addresses and the API are new. There is nothing to migrate, since nothing was ever stored.

### Changed

- Configuration is a TOML file (`--config`) with `TRANSLATION_INFERENCE__SECTION__KEY` environment overrides and `*_FILE` or `file://` secrets, replacing `.env`. See `config.example.toml`.
- The server listens on `127.0.0.1:8080` by default (was `0.0.0.0:3000`). A non-loopback bind needs an https `public_url`; `allow_plain_http = true` opts out on a trusted network.
- `API_BASE_URL`, `API_KEY`, `TRANSLATION_MODEL`, `WHISPER_MODEL`, `TRANSLATION_MODELS` and `WHISPER_MODELS` became `[app.backend]` with `url`, `api_key`, `model`, `transcription_model`, `models` and `transcription_models`. The API key may be left out for endpoints that need none.
- `GATED_ACCESS_KEY` became `app.access_key`, which alone turns on the free and unlocked tiers; `GATED_API_BASE_URL` and `GATED_API_KEY` became the optional `[app.gated]` endpoint. Access keys must be at least 12 characters, and wrong keys are limited to 10 a minute per address.
- `TTS_API_BASE_URL`, `TTS_API_KEY`, `TTS_CHUNK_SIZE` and `TTS_VOICE_MAP` became `[app.speech]` with `url`, `api_key`, `chunk_bytes` and a `voices` table (`en = "voice@model"`).
- `BITVAULT_URL` became `[app.bitvault]`, which talks to BitVault 2026.11: pastes are created through `/api/v1/pastes` and opened by their three-word address. `BITVAULT_API_KEY` became `uploader_password`.
- `DEFAULT_CONTEXT_SIZE`, `INPUT_TOKEN_RATIO`, `CJK_CHARS_PER_TOKEN`, `LATIN_CHARS_PER_TOKEN` and `MIN_OUTPUT_RATIO` moved to `[app.tuning]`; `FREE_TIER_CHAR_LIMIT` and `GATED_CHAR_LIMIT` became `app.free_char_limit` and `app.gated_char_limit`.
- Connecting your own endpoint is off unless `app.byok = true`, because the server then sends requests wherever a visitor points it. The endpoint and key stay on the server for the browser session; they are no longer sent with every request.
- The API moved under `/api/v1` with new request shapes: `/translate`, `/translate/stream` (server-sent events instead of a chunked body with an error sentinel), `/translate/paragraphs`, `/detect`, `/transcribe` (one file per request), `/subtitles`, `/speech`, `/bitvault`, `/models`, `/languages` and `/health`. Errors are JSON `{"error": {"code", "message"}}`.
- The interface follows the manifest: Text, Files, Talk and Settings as pages with a tab bar on phones, the manifest's colours, type and icons in light and dark, and a strict content security policy. Model choice, the access key and your own endpoint are set in Settings.
- Markdown is rendered on the server, and raw HTML in a translation is shown as text.
- Releases ship static musl binaries for x86_64 and aarch64 as offline bundles with `install.sh`, a hardened systemd unit, an nginx template and checksums. macOS and Windows builds are no longer published.

### Added

- Every target language can be typed by name, besides the listed ones.
- An API reference at `/docs`, rendered on the server from `/api/v1/openapi.json`, with a `curl` line for each call.
- Per-address rate limits on the API: 120 requests a minute with a burst of 30.
- Security headers on every response, host and origin checks, and request body limits.
- Undo after clearing the text or the conversation.
- A streamed translation that hit the model's output limit is marked Incomplete.

### Removed

- The Swagger UI and the hand-written `openapi.yaml`.
- `COOKIE_SECURE`: the session cookie is always `Secure`, except with `allow_plain_http`.
- Per-request `endpoint`, `api_key`, `stt_endpoint`, `tts_endpoint` and their keys in API bodies.
- `/api/proxy-text`: `/?from=` opens the paste on the server.
- The changing page title.
