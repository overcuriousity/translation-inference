# translation-inference
<img width="1087" height="794" alt="image" src="https://github.com/user-attachments/assets/b5665388-d45f-42ad-971d-81ba721310fc" />

A Rust-based translation and transcription inference service.

## Features

- **Translation:** Translate text between multiple languages.
- **Files**: Input docx, odt, PDF, get out translated odt, pdf, docx
- **Transcription:** Transcribe audio using Whisper-compatible APIs.
- **Web Interface:** Includes a built-in static web interface for easy interaction.
- **API:** RESTful API for integration with other services.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- **PDF Support (optional):**
  - `poppler-utils` (for `pdftotext`)
  - `paps` (for text-to-PDF conversion)

## Setup

1. Clone the repository:
   ```bash
   git clone https://github.com/your-username/translation-inference.git
   cd translation-inference
   ```

2. Copy the example environment file and configure your settings:
   ```bash
   cp .env.example .env
   ```

3. Run the application:
   ```bash
   cargo run
   ```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
