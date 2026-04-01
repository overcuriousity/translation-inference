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
- `POST /api/translate` - Translate raw text.
- `POST /api/translate/stream` - Translate raw text with SSE streaming response.
- `POST /api/transcribe` - Transcribe an audio or video file.
- `POST /api/translate-document` - Translate `.docx`, `.odt`, or `.pdf` files.
- `POST /api/upload` - Unified upload endpoint for mixed media (transcribes audio/video, translates documents).
- `POST /api/save-to-bitvault` - *(requires `BITVAULT_URL`)* Save text as a Bitvault paste and return its URL.
- `GET /api/proxy-text?url=<raw-url>` - *(requires `BITVAULT_URL`)* Proxy raw text from a Bitvault URL (used by the `?from=` preload feature to avoid CORS).

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
