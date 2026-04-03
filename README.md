# translation-inference

<p align="center">
  <img width="1087" height="794" alt="translation-inference web interface" src="https://github.com/user-attachments/assets/b5665388-d45f-42ad-971d-81ba721310fc" />
</p>

A fast, memory-efficient, Rust-based translation and transcription inference service with a built-in modern web interface. Designed to act as a unified frontend and API for any OpenAI-compatible LLM and Whisper endpoint (such as Ollama, vLLM, LM Studio, or LiteLLM).

## ✨ Features

- **Bring Your Own API**: Seamlessly integrates with any OpenAI-compatible backend for both LLMs (translation) and Whisper models (transcription).
- **Document Translation**: Upload and translate full documents while preserving formatting. Supports `.docx`, `.odt`, and `.pdf` files.
- **Audio & Video Transcription**: Extract and transcribe speech from audio and video files (`.mp3`, `.wav`, `.mp4`, `.mkv`, etc.).
  - **Memory-Efficient**: Uploads are streamed directly to disk. Large files (up to 100MB) are automatically chunked (25MB segments) using `ffmpeg` without exhausting server RAM.
- **Real-Time Streaming**: Text translation supports streaming outputs for a responsive UI experience.
- **Modern Web Interface**: A clean, built-in static web UI. Supports session-based credential storage (bring your own API key directly in the browser).
- **Auto-Model Fetching**: Automatically fetches available models from the connected endpoint.
- **Gated Tier** *(optional)*: A second server-side backend protected by a shared access key. Users enter the access key in the UI to unlock the pre-configured backend — the actual LLM credentials never leave the server. Useful for shared deployments where you want to expose a curated model without distributing the API key.
- **Text-to-Speech** *(optional)*: Read translated text aloud via any OpenAI-compatible `/v1/audio/speech` endpoint. Works with hosted APIs (e.g. OpenAI) and self-hosted local models (e.g. Qwen3-TTS). Users can also supply their own TTS endpoint and key directly in the browser (BYOK). Long texts are automatically split into chunks before synthesis.
- **Bitvault Integration** *(optional)*: Save source or translated text as Bitvault pastes directly from the UI, and preload source text from a Bitvault raw URL via the `?from=` query parameter.

## 🛠️ Prerequisites

Before you begin, ensure you have the following installed:

- **[Rust](https://www.rust-lang.org/tools/install)** (latest stable version)
- **`ffmpeg`**: Required for extracting audio from video files and chunking large audio files.
  - Ubuntu/Debian: `sudo apt install ffmpeg`
  - macOS: `brew install ffmpeg`
- **PDF Support (Optional but recommended):**
  - `poppler-utils` (provides `pdftotext` for PDF text extraction)
    - Ubuntu/Debian: `sudo apt install poppler-utils`
    - Fedora/RHEL: `sudo dnf install poppler-utils`
    - macOS: `brew install poppler`
  - `liberation-fonts` for PDF output rendering
    - Ubuntu/Debian: `sudo apt install fonts-liberation`
    - Fedora/RHEL: `sudo dnf install liberation-fonts`
    - macOS: `brew install --cask font-liberation`

## 🚀 Getting Started

1. **Clone the repository:**
   ```bash
   git clone https://github.com/overcuriousity/translation-inference.git
   cd translation-inference
   ```

2. **Configure Environment:**
   Copy the example environment file and adjust it to your OpenAI-compatible endpoint.
   ```bash
   cp .env.example .env
   ```
   *Note: You can also leave the `.env` empty and provide your API endpoint and key dynamically via the Web UI.*

3. **Run the server:**
   ```bash
   cargo run --release
   ```
   The server will start on `http://0.0.0.0:3000` by default.

## ⚙️ Configuration (`.env`)

```env
# Your OpenAI-compatible API base URL
API_BASE_URL=https://llm.mikoshi.de
API_KEY=your-api-key-here

# Default models (used if none are selected in the UI)
TRANSLATION_MODEL=gpgpu/qwen3:14b-q5_k_m-32768
WHISPER_MODEL=gpgpu/whisper

# Optional: Comma-separated list of models to show in the UI.
# If left empty, the server automatically fetches available models from the API.
TRANSLATION_MODELS=gpgpu/qwen3:14b-q5_k_m-32768,deepseek-chat
WHISPER_MODELS=gpgpu/whisper

# Server bind address
LISTEN_ADDR=0.0.0.0:3000

# Optional: TTS (text-to-speech) — enables the speaker button in the output panel.
# Point at any OpenAI-compatible /v1/audio/speech endpoint.
# TTS_MODEL defaults to "tts-1"; TTS_VOICE defaults to "alloy".
# TTS_CHUNK_SIZE: max bytes per synthesis request (unset/0 = no chunking, recommended for local models).
# TTS_API_BASE_URL=http://tts.example.com
# TTS_API_KEY=your-tts-api-key-here
# TTS_MODEL=Qwen3-TTS
# TTS_VOICE=alloy
# TTS_CHUNK_SIZE=0

# Optional: Gated tier — a second backend protected by an access key.
# Users must enter GATED_ACCESS_KEY in the UI to unlock this tier.
# The actual LLM credentials (GATED_API_BASE_URL, GATED_API_KEY) stay server-side.
# All three must be set for the gated tier to be enabled.
# GATED_API_BASE_URL=https://premium-llm.example.com
# GATED_API_KEY=your-gated-backend-key
# GATED_ACCESS_KEY=shared-password-for-users

# Optional: Set to "1" or "true" to add the Secure flag to session cookies.
# Enable this when serving over HTTPS.
# COOKIE_SECURE=true

# Optional: Bitvault integration
# Enables "Save to Bitvault" buttons in the UI and ?from=<raw-url> source preloading.
# BITVAULT_URL must point to the root of your Bitvault instance (no trailing slash).
# BITVAULT_API_KEY is only required if your instance enforces authentication.
# BITVAULT_URL=https://paste.example.com
# BITVAULT_API_KEY=your-bitvault-api-key-here
```

## 📡 API Endpoints

The service provides a RESTful API for integrations:

- `GET /api/status` - Check server configuration and session status.
- `GET /api/models` - Fetch available translation and transcription models.
- `POST /api/config/test` - Validate a BYOK endpoint+key pair and set a session cookie on success.
- `POST /api/config/gated` - *(requires `GATED_ACCESS_KEY` configured)* Authenticate with the shared access key to obtain a session cookie for the server-side gated backend. Request body: `{ "access_key": "..." }`.
- `POST /api/translate` - Translate raw text.
- `POST /api/translate/stream` - Translate raw text with SSE streaming response.
- `POST /api/transcribe` - Transcribe an audio or video file.
- `POST /api/translate-document` - Translate `.docx`, `.odt`, or `.pdf` files.
- `POST /api/upload` - Unified upload endpoint for mixed media (transcribes audio/video, translates documents).
- `POST /api/tts` - Synthesize translated text to speech. Requires `TTS_API_BASE_URL` to be configured server-side, or `tts_endpoint`+`tts_api_key` in the request body (BYOK). Request body: `{ "text": "...", "tts_endpoint": "...", "tts_api_key": "..." }`. Returns `audio/mpeg`.
- `POST /api/save-to-bitvault` - *(requires `BITVAULT_URL`)* Save text as a Bitvault paste and return its URL.
- `GET /api/proxy-text?url=<raw-url>` - *(requires `BITVAULT_URL`)* Proxy raw text from a Bitvault URL (used by the `?from=` preload feature to avoid CORS).

> **Note:** Translation/transcription endpoints accept either a session cookie (set via `/api/config/test` or `/api/config/gated`) or direct calls with `Authorization: Bearer <GATED_ACCESS_KEY>`. When `GATED_ACCESS_KEY` is configured, all direct API requests (no session cookie) must include this header — including BYOK calls that supply their own `endpoint`+`api_key`. If `GATED_ACCESS_KEY` is not configured, direct API access is disabled entirely and only the web interface can be used.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
