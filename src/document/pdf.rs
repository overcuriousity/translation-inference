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

/// Translate a PDF by extracting its text, translating it, and generating a new PDF using `paps`.
pub async fn translate_pdf(
    bytes: &[u8],
    client: &crate::api::client::OpenAiClient,
    model: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<Vec<u8>> {
    let text = extract_pdf_text(bytes).context("failed to extract text from PDF")?;
    
    let (translated, _, _) = crate::api::chat::translate(client, model, source_lang, target_lang, &text)
        .await
        .context("translation failed")?;

    // Create a temp file for the translated text
    let mut txt_tmp = tempfile::Builder::new()
        .suffix(".txt")
        .tempfile()
        .context("failed to create temp file for translated text")?;
    txt_tmp.write_all(translated.as_bytes()).context("failed to write translated text")?;
    txt_tmp.flush()?;

    // Use `paps` to convert text to PDF
    let output = Command::new("paps")
        .args([
            "--format=pdf",
            "--font=Monospace 10",
            "--paper=a4",
            txt_tmp.path().to_str().unwrap_or(""),
        ])
        .output()
        .context("failed to run paps — is it installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("paps failed: {stderr}");
    }

    Ok(output.stdout)
}

/// Check whether `pdftotext` and `paps` are available in PATH.
pub fn is_available() -> bool {
    let pdf_to_text = Command::new("pdftotext")
        .arg("-v")
        .output()
        .map(|o| o.status.success() || !o.stderr.is_empty())
        .unwrap_or(false);

    let paps = Command::new("paps")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    pdf_to_text && paps
}
