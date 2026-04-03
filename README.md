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
- **Text-to-Speech** *(optional)*: Read translated text aloud — or read the source text back — via any OpenAI-compatible `/v1/audio/speech` endpoint. Works with hosted APIs (e.g. OpenAI) and self-hosted local models. The recommended self-hosted option is **[speaches-ai](https://speaches.ai)**, which can serve both Kokoro-82M (en, es, fr, hi, it, pt) and Piper models (de, ru, zh, ar, cs, da, el, fi, hu, nl, no, pl, ro, sv, tr, uk, and more) from the same instance. `TTS_VOICE_MAP` selects the right model and voice per language; see the configuration section for a comprehensive example. Users can also supply their own TTS endpoint and key directly in the browser (BYOK). Long texts are automatically split into chunks before synthesis.
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

# ── TTS (text-to-speech) ─────────────────────────────────────────────────────
# Enables speaker buttons in the source and output panels.
# Point TTS_API_BASE_URL at any OpenAI-compatible /v1/audio/speech endpoint.
#
# Recommended: speaches-ai (https://speaches.ai) serving both Kokoro-82M and
# Piper models.  TTS_VOICE_MAP routes each language to the right model+voice.
#
# TTS_VOICE_MAP  (replaces TTS_MODEL / TTS_VOICE)
#   Comma-separated entries: lang:voice@model
#   • lang   ISO 639-1 code (zh-TW for Traditional Chinese, zh for Simplified)
#   • voice  voice ID accepted by the model
#   • @model model ID override (omit to fall back to TTS_MODEL)
#   Languages listed here get TTS buttons in the UI; unlisted ones do not.
#
# TTS_CHUNK_SIZE: max bytes per request (0/unset = no chunking; 4000 = OpenAI limit)
#
# TTS_MODEL / TTS_VOICE are deprecated; prefer TTS_VOICE_MAP entries with @model.
#
# ── speaches-ai voice reference ──────────────────────────────────────────────
#
#  Kokoro-82M-v1.0-ONNX-fp16  (speaches-ai/Kokoro-82M-v1.0-ONNX-fp16)
#   en (US) ♀  af_heart · af_alloy · af_aoede · af_bella · af_jessica · af_kore
#              af_nicole · af_nova · af_river · af_sarah · af_sky
#   en (US) ♂  am_adam · am_echo · am_eric · am_fenrir · am_liam · am_michael
#              am_onyx · am_puck · am_santa
#   en (GB) ♀  bf_alice · bf_emma · bf_isabella · bf_lily
#   en (GB) ♂  bm_daniel · bm_fable · bm_george · bm_lewis
#   es ♀  ef_dora          es ♂  em_alex · em_santa
#   fr ♀  ff_siwis
#   hi ♀  hf_alpha · hf_beta   hi ♂  hm_omega · hm_psi
#   it ♀  if_sara          it ♂  im_nicola
#   pt ♀  pf_dora          pt ♂  pm_alex · pm_santa
#   zh Kokoro voices (zf_*, zm_*): BROKEN — speaches-ai passes "zh" to espeak
#      which expects "cmn"; use the Piper zh model below instead.
#   ja Kokoro voices (jf_*, jm_*): BROKEN — espeak switches phoneme sets
#      mid-utterance; no Piper ja model available in speaches-ai.
#
#  Piper models  (single voice per model)
#   ar   kareem       speaches-ai/piper-ar_JO-kareem-medium
#   cs   jirka        speaches-ai/piper-cs_CZ-jirka-medium
#   da   talesyntese  speaches-ai/piper-da_DK-talesyntese-medium
#   de   thorsten     speaches-ai/piper-de_DE-thorsten-high
#   el   rapunzelina  speaches-ai/piper-el_GR-rapunzelina-low
#   fi   harri        speaches-ai/piper-fi_FI-harri-medium
#   hu   anna         speaches-ai/piper-hu_HU-anna-medium
#   nl   mls          speaches-ai/piper-nl_NL-mls-medium
#   no   talesyntese  speaches-ai/piper-no_NO-talesyntese-medium
#   pl   darkman      speaches-ai/piper-pl_PL-darkman-medium
#   ro   mihai        speaches-ai/piper-ro_RO-mihai-medium
#   ru   ruslan       speaches-ai/piper-ru_RU-ruslan-medium
#   sv   nst          speaches-ai/piper-sv_SE-nst-medium
#   tr   dfki         speaches-ai/piper-tr_TR-dfki-medium
#   uk   lada         speaches-ai/piper-uk_UA-lada-x_low
#   zh   huayan       speaches-ai/piper-zh_CN-huayan-medium  (Simplified + Traditional)
#
#  No TTS support: bg (Bulgarian), ko (Korean), ja (Japanese)
# ─────────────────────────────────────────────────────────────────────────────
#
# TTS_API_BASE_URL=http://tts.example.com
# TTS_API_KEY=your-tts-api-key-here
# TTS_CHUNK_SIZE=0
# TTS_VOICE_MAP=\
#   en:af_heart@speaches-ai/Kokoro-82M-v1.0-ONNX-fp16,\
#   es:ef_dora@speaches-ai/Kokoro-82M-v1.0-ONNX-fp16,\
#   fr:ff_siwis@speaches-ai/Kokoro-82M-v1.0-ONNX-fp16,\
#   hi:hf_alpha@speaches-ai/Kokoro-82M-v1.0-ONNX-fp16,\
#   it:if_sara@speaches-ai/Kokoro-82M-v1.0-ONNX-fp16,\
#   pt:pf_dora@speaches-ai/Kokoro-82M-v1.0-ONNX-fp16,\
#   ar:kareem@speaches-ai/piper-ar_JO-kareem-medium,\
#   cs:jirka@speaches-ai/piper-cs_CZ-jirka-medium,\
#   da:talesyntese@speaches-ai/piper-da_DK-talesyntese-medium,\
#   de:thorsten@speaches-ai/piper-de_DE-thorsten-high,\
#   el:rapunzelina@speaches-ai/piper-el_GR-rapunzelina-low,\
#   fi:harri@speaches-ai/piper-fi_FI-harri-medium,\
#   hu:anna@speaches-ai/piper-hu_HU-anna-medium,\
#   nl:mls@speaches-ai/piper-nl_NL-mls-medium,\
#   no:talesyntese@speaches-ai/piper-no_NO-talesyntese-medium,\
#   pl:darkman@speaches-ai/piper-pl_PL-darkman-medium,\
#   ro:mihai@speaches-ai/piper-ro_RO-mihai-medium,\
#   ru:ruslan@speaches-ai/piper-ru_RU-ruslan-medium,\
#   sv:nst@speaches-ai/piper-sv_SE-nst-medium,\
#   tr:dfki@speaches-ai/piper-tr_TR-dfki-medium,\
#   uk:lada@speaches-ai/piper-uk_UA-lada-x_low,\
#   zh:huayan@speaches-ai/piper-zh_CN-huayan-medium,\
#   zh-TW:huayan@speaches-ai/piper-zh_CN-huayan-medium

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
- `POST /api/detect-language` - Detect the language of a text snippet (up to 500 chars). Uses the same translation LLM and auth as `/api/translate`. Request body: `{ "text": "...", "endpoint": "...", "api_key": "..." }` (`endpoint`+`api_key` are optional BYOK overrides, same as `/api/translate`). Returns `{ "language": "en" }` (ISO 639-1 code; Traditional Chinese returns `zh-TW`).
- `POST /api/save-to-bitvault` - *(requires `BITVAULT_URL`)* Save text as a Bitvault paste and return its URL.
- `GET /api/proxy-text?url=<raw-url>` - *(requires `BITVAULT_URL`)* Proxy raw text from a Bitvault URL (used by the `?from=` preload feature to avoid CORS).

> **Note:** All endpoints accept a session cookie (set via `/api/config/test` or `/api/config/gated`) or a `Authorization: Bearer <GATED_ACCESS_KEY>` header. When `GATED_ACCESS_KEY` is configured, unauthenticated requests (no cookie, no Bearer token) are still allowed as a **free tier** as long as the server has its own API credentials — matching the behaviour of the web interface. If `GATED_ACCESS_KEY` is not configured (personal/local mode), direct API access is open when the server is configured.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
