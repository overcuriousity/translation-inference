# AGENTS.md

translation-inference is a self-hosted translator, transcriber and two-person interpreter in front of OpenAI-compatible model endpoints, for people who run their own server or share one with a few others.

## Rules

Follows overcuriousity dev manifest v2026.11.1 (principles and stack; no oc-kit dependency):
https://github.com/overcuriousity/overcuriousity-dev-manifest/tree/v2026.11.1/manifest

- Profile: everyday.
- Read the manifest files in number order before changing UI, security or ops.
- Before declaring work done, run `manifest/99-review-checklist.md` by hand and `oc-check .`.
- The design files in `assets/vendor/oc/` are copied unmodified from the manifest; `assets/vendor/oc/SOURCE.md` names the tag and source path of each. Updating means a deliberate re-copy from a newer tag.
- The security layer (`security.rs`, `ratelimit.rs`, `deployment.rs`), config, sessions and page shell are this app's own.

## Deliberate exceptions to the manifest

1. No database and no accounts (`50-stack.md` SQLite, sessions, sign-in). Nothing is stored: sessions live in memory and hold only what a visitor unlocked, chose or connected; a restart forgets them. Access is one shared key (`app.access_key`), checked in constant time and rate-limited per address.
2. Theme and text size are kept in `localStorage`, not server-side (`10-visual.md` "store the choice per user"): there are no users to store them for.
3. The CSP adds `media-src 'self' blob:` (`60-security.md` "exactly this CSP"). Without it the browser refuses to play the speech the server synthesises.
4. No MCP endpoint (`50-stack.md` rmcp at `/mcp`). The REST API covers every function.
5. No PWA (`30-layout.md` SHOULD): the app is useless offline, since every function needs a model endpoint.
6. With `app.byok`, off by default, the server sends requests to an endpoint a visitor enters (`60-security.md` "no egress unless configured"). The deployer turns it on knowingly; `config.example.toml` says so.
7. No htmx: no page swaps fragments. Forms post and redirect; the translator, files and talk pages are small ES modules over the JSON API.

## Layout

- `src/domain/`: translation (chunking, prompts, streaming), transcription (ffmpeg, segments), speech, subtitles, detection, BitVault, Markdown, upstream endpoints and model probing.
- `src/api/`: the REST API under `/api/v1` (utoipa); `src/web/`: Askama pages, Settings, the `/docs` reference rendered from the OpenAPI document.
- `src/access.rs` tiers and limits; `src/session.rs` in-memory sessions; `src/security.rs`, `src/ratelimit.rs`, `src/deployment.rs`, `src/config.rs`, `src/assets.rs`.
- `templates/` Askama templates; `assets/css`, `assets/js` the app's own; `assets/vendor/oc/` copied files.
- `tests/` integration tests against an in-process fake OpenAI endpoint.
- `deploy/`, `install.sh`, `scripts/` ops; `config.example.toml` documents every key.

## Commands

- `cargo fmt --all --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `node --test 'assets/js/*.test.mjs'`
- `cargo deny check`
- `oc-check .`
- `sh scripts/third-party.sh > THIRD-PARTY-LICENSES` after dependency changes (needs `cargo-about`).
