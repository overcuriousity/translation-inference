use anyhow::{Context, Result};
use std::io::Write;
use std::process::Command;

/// Extract plain text from a PDF using `pdftotext` (poppler-utils).
pub fn extract_pdf_text(bytes: &[u8]) -> Result<String> {
    let mut tmp = tempfile::Builder::new()
        .suffix(".pdf")
        .tempfile()
        .context("failed to create temp file for PDF")?;
    tmp.write_all(bytes).context("failed to write PDF bytes")?;
    tmp.flush()?;

    let output = Command::new("pdftotext")
        .args(["-layout", tmp.path().to_str().unwrap_or(""), "-"])
        .output()
        .context("failed to run pdftotext — is poppler-utils installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("pdftotext failed: {stderr}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Translate a PDF by extracting its text, translating it, and generating a new PDF.
pub async fn translate_pdf(
    bytes: &[u8],
    client: &crate::api::client::OpenAiClient,
    model: &str,
    source_lang: &str,
    target_lang: &str,
    config: &crate::api::chunker::TranslationConfig,
) -> Result<Vec<u8>> {
    let text = extract_pdf_text(bytes).context("failed to extract text from PDF")?;

    let (translated, _, _) =
        crate::api::chat::translate(client, model, source_lang, target_lang, &text, config)
            .await
            .context("translation failed")?;

    build_pdf_from_text(&translated)
}

/// Render plain text into a PDF using `genpdf` (pure Rust, no system dependencies).
pub fn build_pdf_from_text(text: &str) -> Result<Vec<u8>> {
    let font = genpdf::fonts::from_files("/usr/share/fonts/liberation-mono", "LiberationMono", None)
        .or_else(|_| genpdf::fonts::from_files("/usr/share/fonts/truetype/liberation", "LiberationMono", None))
        .or_else(|_| genpdf::fonts::from_files("/usr/share/fonts/liberation", "LiberationMono", None))
        .context("failed to load font — install liberation-fonts or liberation-fonts-ttf")?;

    let mut doc = genpdf::Document::new(font);
    doc.set_title("Translated document");
    doc.set_minimal_conformance();
    doc.set_line_spacing(1.25);

    let mut decorator = genpdf::SimplePageDecorator::new();
    decorator.set_margins(15);
    doc.set_page_decorator(decorator);

    for line in text.lines() {
        doc.push(genpdf::elements::Paragraph::new(line));
    }

    let mut buf = Vec::new();
    doc.render(&mut buf).context("failed to render PDF")?;
    Ok(buf)
}

/// Check whether `pdftotext` is available in PATH.
pub fn is_available() -> bool {
    Command::new("pdftotext")
        .arg("-v")
        .output()
        .map(|o| o.status.success() || !o.stderr.is_empty())
        .unwrap_or(false)
}
