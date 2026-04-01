# translation-inference

A Rust-based translation and transcription inference service.

## Features

- **Translation:** Translate text between multiple languages.
- **Transcription:** Transcribe audio using Whisper-compatible APIs.
- **Web Interface:** Includes a built-in static web interface for easy interaction.
- **API:** RESTful API for integration with other services.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- GitHub CLI (optional, for deployment)

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
