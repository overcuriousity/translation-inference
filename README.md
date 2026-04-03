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
- **Text-to-Speech** *(optional)*: Read translated text aloud — or read the source text back — via any OpenAI-compatible `/v1/audio/speech` endpoint. Works with hosted APIs (e.g. OpenAI) and self-hosted local models. The recommended self-hosted option is **[speaches-ai](https://speaches.ai)** running the Kokoro-82M-v1.0-ONNX model, which provides multilingual voices for en, es, fr, hi, it, and pt. Known limitations with the current speaches-ai/espeak backend: zh voices crash (espeak expects `cmn`, not `zh`); ja voices produce partial audio due to espeak switching phoneme sets mid-utterance — both require a native phonemizer (pyopenjtalk/MeCab) to be fixed upstream. Per-language voice selection is supported via `TTS_VOICE_MAP`. Users can also supply their own TTS endpoint and key directly in the browser (BYOK). Long texts are automatically split into chunks before synthesis.
- **Automatic Source Language Detection**: When text is entered, the source language is detected automatically via a lightweight LLM inference call (triggered immediately on paste, or after a 500 ms typing pause). The detected language is shown as a badge in the source panel and enables the source TTS button.
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

# Optional: TTS (text-to-speech) — enables speaker buttons in the source and output panels.
# Point at any OpenAI-compatible /v1/audio/speech endpoint.
# Recommended self-hosted option: speaches-ai (https://speaches.ai) with Kokoro-82M-v1.0-ONNX.
# Kokoro natively supports en, es, fr, hi, it, ja, pt, zh.
# All other languages fall back to TTS_VOICE.
# TTS_MODEL: model ID your endpoint expects.
# TTS_VOICE: default voice (fallback when target language has no TTS_VOICE_MAP entry).
# TTS_CHUNK_SIZE: max bytes per synthesis request (unset/0 = no chunking).
#   Set to 4000 to match the OpenAI hosted-API limit.
# TTS_VOICE_MAP: per-language voice overrides — comma-separated lang:voice pairs.
#   The frontend sends ISO 639-1 codes (en, fr, es, ja, zh, ...) as the language key.
#
#   Kokoro-82M-v1.0-ONNX working voices (confirmed against phonemizer/espeak):
#     en (US) female: af_heart, af_alloy, af_aoede, af_bella, af_jessica, af_kore,
#                     af_nicole, af_nova, af_river, af_sarah, af_sky
#     en (US) male:   am_adam, am_echo, am_eric, am_fenrir, am_liam, am_michael,
#                     am_onyx, am_puck, am_santa
#     en (GB) female: bf_alice, bf_emma, bf_isabella, bf_lily
#     en (GB) male:   bm_daniel, bm_fable, bm_george, bm_lewis
#     es female:      ef_dora      | es male:   em_alex, em_santa
#     fr female:      ff_siwis
#     hi female:      hf_alpha, hf_beta  | hi male: hm_omega, hm_psi
#     it female:      if_sara      | it male:   im_nicola
#     ja [BROKEN]: espeak switches phoneme sets mid-utterance even within a single
#                  sentence; omit from TTS_VOICE_MAP until speaches-ai uses pyopenjtalk.
#     pt female:      pf_dora      | pt male:   pm_alex, pm_santa
#     zh [BROKEN]: Kokoro passes lang code "zh" to espeak but espeak expects "cmn";
#                  omit from TTS_VOICE_MAP until fixed upstream in speaches-ai.
#
# TTS_API_BASE_URL=http://tts.example.com
# TTS_API_KEY=your-tts-api-key-here
# TTS_MODEL=speaches-ai/Kokoro-82M-v1.0-ONNX-fp16
# TTS_VOICE=af_heart
# TTS_CHUNK_SIZE=0
# TTS_VOICE_MAP=en:af_heart,es:ef_dora,fr:ff_siwis,hi:hf_alpha,it:if_sara,pt:pf_dora

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
- `POST /api/tts` - Synthesize text to speech. Requires `TTS_API_BASE_URL` to be configured server-side, or `tts_endpoint`+`tts_api_key` in the request body (BYOK). Request body: `{ "text": "...", "target_lang": "en", "tts_endpoint": "...", "tts_api_key": "..." }`. Returns `audio/mpeg`.
- `POST /api/detect-language` - Detect the language of a text snippet (up to 500 chars). Uses the same translation LLM and auth as `/api/translate`. Request body: `{ "text": "..." }`. Returns `{ "language": "en" }` (ISO 639-1 code).
- `POST /api/save-to-bitvault` - *(requires `BITVAULT_URL`)* Save text as a Bitvault paste and return its URL.
- `GET /api/proxy-text?url=<raw-url>` - *(requires `BITVAULT_URL`)* Proxy raw text from a Bitvault URL (used by the `?from=` preload feature to avoid CORS).

> **Note:** All endpoints accept a session cookie (set via `/api/config/test` or `/api/config/gated`) or a `Authorization: Bearer <GATED_ACCESS_KEY>` header. When `GATED_ACCESS_KEY` is configured, unauthenticated requests (no cookie, no Bearer token) are still allowed as a **free tier** as long as the server has its own API credentials — matching the behaviour of the web interface. If `GATED_ACCESS_KEY` is not configured (personal/local mode), direct API access is open when the server is configured.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
