# translation-inference

<img width="1112" height="958" alt="image" src="https://github.com/user-attachments/assets/bfc89dd0-3b50-47a4-ae1c-d60023ade26f" />


A fast, memory-efficient, Rust-based translation and transcription inference service with a built-in modern web interface. Designed to act as a unified frontend and API for any OpenAI-compatible LLM and Whisper endpoint (such as llama.cpp, vLLM, LM Studio, or LiteLLM).

## ✨ Features

- **Bring Your Own API**: Seamlessly integrates with any OpenAI-compatible backend for LLMs (translation) and Whisper models (transcription).
- **Three-Tier Access Model**: Free (anonymous web), gated (access-key), and BYOK (user-supplied credentials) — see [Access Tiers](#access-tiers).
- **Audio & Video Transcription**: Extract and transcribe speech from audio and video files (`.mp3`, `.wav`, `.mp4`, `.mkv`, etc.).
  - **Memory-Efficient**: Uploads are streamed directly to disk. Large files (up to 100 MB) are automatically chunked (25 MB segments) using `ffmpeg` without exhausting server RAM.
- **Real-Time Streaming**: Text translation supports streaming outputs for a responsive UI experience.
- **Modern Web Interface**: A clean, built-in static web UI. The header shows the active backend at a glance (endpoint hostname and model); full model selection and credentials live in the settings panel.
- **Auto-Model Fetching**: Automatically fetches available models from the connected endpoint.
- **Text-to-Speech** *(optional)*: Read translated text aloud — or read the source text back — via any OpenAI-compatible `/v1/audio/speech` endpoint. Works with hosted APIs (e.g. OpenAI) and self-hosted local models. `TTS_VOICE_MAP` routes each language to the right voice and model. Long texts are automatically split into chunks before synthesis.
- **Automatic Source Language Detection**: Source language is detected automatically via a lightweight LLM call (triggered on paste or after a 500 ms typing pause). Shown as a badge in the source panel.
- **Subtitle Translation** *(gated/BYOK)*: Upload `.srt` or `.vtt` subtitle files and receive a translated file in the same format, streamed via SSE.
- **Bitvault Integration** *(optional)*: Save source or translated text as Bitvault pastes directly from the UI, and preload source text from a Bitvault raw URL via the `?from=` query parameter.

## 🛠️ Prerequisites

- **[Rust](https://www.rust-lang.org/tools/install)** (latest stable)
- **`ffmpeg`**: Required for audio/video transcription and chunking.
  - Ubuntu/Debian: `sudo apt install ffmpeg`
  - macOS: `brew install ffmpeg`

## 🚀 Getting Started

1. **Clone the repository:**
   ```bash
   git clone https://github.com/overcuriousity/translation-inference.git
   cd translation-inference
   ```

2. **Configure:**
   ```bash
   cp .env.example .env
   # Edit .env to point at your LLM endpoint
   ```

3. **Run:**
   ```bash
   cargo run --release
   ```
   The server starts on `http://0.0.0.0:3000` by default.

---

## 🔐 Access Tiers

The server supports three access tiers controlled by environment variables. All three tiers are active simultaneously when `GATED_ACCESS_KEY` is configured.

### Tier 1 — Free (anonymous web)

- **How it works**: A session cookie is issued automatically when the browser loads the page. No credentials are required.
- **What's available**: Text translation, conversation mode (voice-to-voice), TTS (if configured).
- **Restrictions**: The file/subtitle tab is hidden. A configurable per-request character limit applies (default ~16 000 chars ≈ 4 096 tokens). Direct REST API calls without a session cookie are rejected with HTTP 401.
- **Backend used**: The server's own `API_BASE_URL`/`API_KEY`.
- **Enabled when**: `API_BASE_URL`, `API_KEY`, and `GATED_ACCESS_KEY` are all set.

### Tier 2 — Gated (shared access key)

- **How it works**: Users enter `GATED_ACCESS_KEY` in the settings panel, or send it as `Authorization: Bearer <key>` in REST API requests.
- **What's available**: Full UI including file/subtitle tab. Full REST API.
- **Restrictions**: An optional per-request character limit applies (default ~65 536 chars ≈ 16 384 tokens; configurable).
- **Backend used**: `GATED_API_BASE_URL`/`GATED_API_KEY` (typically a higher-quota or more capable model).
- **Enabled when**: `GATED_API_BASE_URL`, `GATED_API_KEY`, and `GATED_ACCESS_KEY` are all set.

### Tier 3 — BYOK (bring your own key)

- **How it works**: Users enter their own endpoint URL and API key in the settings panel. Credentials are tested via `/api/config/test` and stored in an in-memory session cookie for the browser session.
- **What's available**: Full UI and full REST API. The app acts as a pure proxy — no server-side credentials are used.
- **Restrictions**: None. No character limit is enforced.
- **Backend used**: Whatever endpoint the user provides.

### Personal / local mode

When `GATED_ACCESS_KEY` is **not** set, the server operates in open mode: all API endpoints are accessible without authentication, using the server's own `API_BASE_URL`/`API_KEY`. Suitable for local or single-user deployments.

---

## ⚙️ Configuration (`.env`)

```env
# ── Core backend ──────────────────────────────────────────────────────────────
# Your OpenAI-compatible LLM endpoint (used as the free-tier and personal-mode backend).
API_BASE_URL=https://llm.example.com
API_KEY=your-api-key-here

# Default models (used when none is selected in the UI).
TRANSLATION_MODEL=your-model-id
WHISPER_MODEL=your-whisper-model-id

# Optional: comma-separated list of models shown in the UI.
# If empty, the server auto-fetches available models from the API.
TRANSLATION_MODELS=model-a,model-b
WHISPER_MODELS=whisper-model

# Server bind address. Default: 0.0.0.0:3000
LISTEN_ADDR=0.0.0.0:3000


# ── Access tiers ──────────────────────────────────────────────────────────────
# All three must be set to enable gated/free-tier mode (see Access Tiers above).
# GATED_API_BASE_URL=https://premium-llm.example.com
# GATED_API_KEY=your-gated-backend-key
# GATED_ACCESS_KEY=shared-password-for-users


# ── Per-tier input character limits ───────────────────────────────────────────
# Maximum characters accepted per translation request.
# 0 = unlimited. BYOK tier is always unlimited.
# Defaults: ~16 000 chars (free, ≈ 4 096 tokens) / ~65 536 chars (gated, ≈ 16 384 tokens).
# FREE_TIER_CHAR_LIMIT=16000
# GATED_CHAR_LIMIT=65536


# ── TTS (text-to-speech) ──────────────────────────────────────────────────────
# Enables speaker buttons in the source and output panels.
# Point TTS_API_BASE_URL at any OpenAI-compatible /v1/audio/speech endpoint.
# The endpoint hostname is displayed in the header backend-info bar.
#
# TTS_VOICE_MAP  (sole TTS configuration)
#   Comma-separated entries: lang:voice@model  (@model is required)
#   • lang   ISO 639-1 code (zh-TW for Traditional Chinese, zh for Simplified)
#   • voice  voice ID accepted by the model (see your TTS provider's docs)
#   • @model model ID as registered at your API provider
#   Languages listed here get TTS buttons in the UI; unlisted ones do not.
#   Requests for an unlisted language return HTTP 400.
#
# TTS_CHUNK_SIZE: max bytes per request (0/unset = no chunking; 4000 = OpenAI limit)
#
# TTS_API_BASE_URL=https://tts.example.com
# TTS_API_KEY=your-tts-api-key-here
# TTS_CHUNK_SIZE=0
# TTS_VOICE_MAP=en:af_heart@speaches-ai/Kokoro-82M-v1.0-ONNX-fp16,...


# ── Translation quality tuning ────────────────────────────────────────────────
# DEFAULT_CONTEXT_SIZE: fallback context window (tokens) when the model ID
#   does not encode one. Default: 4096.
# DEFAULT_CONTEXT_SIZE=4096

# INPUT_TOKEN_RATIO: fraction of usable tokens allocated to input.
#   Default: 0.5 (50/50 split). Try 0.4 for compact→verbose language pairs.
# INPUT_TOKEN_RATIO=0.5

# CJK_CHARS_PER_TOKEN / LATIN_CHARS_PER_TOKEN: script-aware token estimates.
#   Defaults: 1.5 / 4.0. Adjust if you see over/under-chunking.
# CJK_CHARS_PER_TOKEN=1.5
# LATIN_CHARS_PER_TOKEN=4.0

# MIN_OUTPUT_RATIO: warn when translated output is shorter than this fraction
#   of the input (char count). Default: 0.3. Set to 0.0 to disable.
# MIN_OUTPUT_RATIO=0.3


# ── Bitvault integration ──────────────────────────────────────────────────────
# Enables "Save to Bitvault" buttons and ?from=<raw-url> source preloading.
# BITVAULT_URL must point to the root of your Bitvault instance (no trailing slash).
# BITVAULT_API_KEY is optional; only needed if your instance requires authentication.
# BITVAULT_URL=https://paste.example.com
# BITVAULT_API_KEY=your-bitvault-api-key-here


# ── Security ──────────────────────────────────────────────────────────────────
# Set to "1" or "true" to add the Secure flag to session cookies.
# Enable this when serving over HTTPS.
# COOKIE_SECURE=true
```

---

## 📡 API Endpoints

All endpoints that perform inference require authentication:
- **Session cookie** (`sid`) — set automatically on page load (free tier) or via `/api/config/test` / `/api/config/gated`. (`/api/config/check` validates credentials without modifying the session.)
- **Bearer token** — `Authorization: Bearer <GATED_ACCESS_KEY>` (gated tier only).
- **Personal/local mode** (no `GATED_ACCESS_KEY`): open access when the server has credentials.

| Method | Path | Auth required | Description |
|--------|------|---------------|-------------|
| `GET`  | `/api/status` | No | Server configuration and session status |
| `GET`  | `/api/models` | No | Available translation and transcription models |
| `GET`  | `/api/languages` | No | Supported target languages |
| `POST` | `/api/config/test` | No | Validate a BYOK endpoint+key and set a session cookie |
| `POST` | `/api/config/check` | Yes | Validate an endpoint+key without touching the session (used to overlay BYOK translation on a gated session) |
| `POST` | `/api/config/gated` | No | Authenticate with `GATED_ACCESS_KEY` and set a session cookie |
| `POST` | `/api/translate` | Yes | Translate text (buffered) |
| `POST` | `/api/translate/stream` | Yes | Translate text with SSE streaming |
| `POST` | `/api/translate/paragraphs` | Yes | Translate with paragraph alignment |
| `POST` | `/api/translate-subtitle` | Yes | Translate `.srt`/`.vtt` subtitle files (SSE) |
| `POST` | `/api/transcribe` | Yes | Transcribe audio/video |
| `POST` | `/api/upload` | Yes | Upload audio/video or subtitle files |
| `POST` | `/api/tts` | Yes | Synthesise text to speech (`audio/mpeg`) |
| `POST` | `/api/detect-language` | Yes | Detect the language of a text snippet |
| `POST` | `/api/save-to-bitvault` | Yes | Save text as a Bitvault paste |
| `GET`  | `/api/proxy-text` | Yes | Proxy raw text from a Bitvault URL |

### Character limits

Translation endpoints (`/api/translate`, `/api/translate/stream`, `/api/translate/paragraphs`, `/api/translate-subtitle`) enforce per-tier character limits on the input text:

| Tier | Default limit | Override |
|------|--------------|---------|
| Free | 16 000 chars | `FREE_TIER_CHAR_LIMIT` |
| Gated | 65 536 chars | `GATED_CHAR_LIMIT` |
| BYOK | Unlimited | — |

Set `FREE_TIER_CHAR_LIMIT=0` or `GATED_CHAR_LIMIT=0` to disable the limit for that tier. Requests that exceed the limit receive HTTP 413 with a descriptive error message. The web UI shows a live character counter (`current / limit`) and disables the Translate button when the limit is exceeded.

### Request bodies

**`POST /api/translate`** and **`POST /api/translate/stream`**:
```json
{
  "text": "Text to translate",
  "target_lang": "English",
  "source_lang": "German",
  "model": "optional-model-id",
  "context": "optional domain hint",
  "endpoint": "optional BYOK endpoint",
  "api_key": "optional BYOK key"
}
```
`source_lang` is optional — omit it for automatic source language detection.

**`POST /api/detect-language`**:
```json
{ "text": "up to 500 chars", "endpoint": "...", "api_key": "..." }
```
Returns `{ "language": "en" }` (ISO 639-1; Traditional Chinese returns `zh-TW`).

**`POST /api/tts`**:
```json
{
  "text": "...",
  "target_lang": "en",
  "tts_endpoint": "optional BYOK TTS endpoint",
  "tts_api_key": "optional BYOK TTS key"
}
```
Returns `audio/mpeg`.

---

## 📄 License

MIT — see [LICENSE](LICENSE).
