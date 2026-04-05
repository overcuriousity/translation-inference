use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use tempfile::NamedTempFile;

use crate::api::client::OpenAiClient;
use crate::models::WhisperResponse;

/// Returns true if the file's extension suggests a video container.
pub fn is_video_file(filename: &str) -> bool {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    matches!(
        ext.as_str(),
        "mp4"
            | "mkv"
            | "avi"
            | "mov"
            | "webm"
            | "flv"
            | "wmv"
            | "m4v"
            | "3gp"
            | "ts"
            | "mpeg"
            | "mpg"
            | "rm"
            | "rmvb"
            | "vob"
            | "mts"
            | "m2ts"
            | "divx"
    )
}

/// Extract audio from a video file using ffmpeg.
/// Returns a NamedTempFile containing the extracted WAV audio.
pub fn extract_audio_from_video(input_path: &Path) -> Result<NamedTempFile> {
    let tmp = tempfile::Builder::new()
        .suffix(".wav")
        .tempfile()
        .context("failed to create temp file for audio extraction")?;

    let status = Command::new("ffmpeg")
        .args([
            "-y", // overwrite output
            "-i",
            input_path.to_str().context("invalid input path")?,
            "-vn", // no video
            "-acodec",
            "pcm_s16le", // raw PCM
            "-ar",
            "16000", // 16 kHz (Whisper optimal)
            "-ac",
            "1", // mono
            tmp.path().to_str().context("invalid temp path")?,
        ])
        .output()
        .context("failed to run ffmpeg — is it installed?")?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        anyhow::bail!("ffmpeg failed: {stderr}");
    }

    Ok(tmp)
}

/// Split a long audio file into 10-minute segments using ffmpeg.
/// Returns a Vec of NamedTempFiles, one per segment.
pub fn split_audio_into_segments(input_path: &Path) -> Result<Vec<NamedTempFile>> {
    let dir = tempfile::tempdir().context("failed to create temp dir for audio segments")?;
    let pattern = dir.path().join("segment_%03d.wav");

    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            input_path.to_str().context("invalid input path")?,
            "-f",
            "segment",
            "-segment_time",
            "600", // 10 minutes
            "-acodec",
            "pcm_s16le",
            "-ar",
            "16000",
            "-ac",
            "1",
            "-reset_timestamps",
            "1",
            pattern.to_str().context("invalid pattern path")?,
        ])
        .output()
        .context("failed to run ffmpeg for segmentation")?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        anyhow::bail!("ffmpeg segmentation failed: {stderr}");
    }

    // Collect segments in order
    let mut segments: Vec<NamedTempFile> = Vec::new();
    let mut i = 0usize;
    loop {
        let seg_path = dir.path().join(format!("segment_{i:03}.wav"));
        if !seg_path.exists() {
            break;
        }
        // Move into a NamedTempFile so the file persists after `dir` is dropped
        let mut tmp = tempfile::Builder::new()
            .suffix(".wav")
            .tempfile()
            .context("failed to create temp file for segment")?;
        std::io::copy(
            &mut std::fs::File::open(&seg_path).context("failed to open segment file")?,
            &mut tmp,
        )
        .context("failed to copy segment")?;
        segments.push(tmp);
        i += 1;
    }

    Ok(segments)
}

/// Duration threshold above which we segment the audio (25 MB).
const LONG_AUDIO_BYTES: u64 = 25 * 1024 * 1024; // 25 MB

/// Send a single audio file to the Whisper API and return the transcription.
pub async fn transcribe_file(
    client: &OpenAiClient,
    whisper_model: &str,
    language: Option<&str>,
    file_path: &Path,
    filename: &str,
) -> Result<String> {
    let mime = mime_guess::from_path(filename)
        .first_or(mime_guess::mime::APPLICATION_OCTET_STREAM)
        .to_string();

    let file = tokio::fs::File::open(file_path)
        .await
        .context("failed to open file for upload")?;
    let stream = tokio_util::io::ReaderStream::new(file);

    let part = reqwest::multipart::Part::stream(reqwest::Body::wrap_stream(stream))
        .file_name(filename.to_string())
        .mime_str(&mime)
        .context("failed to set MIME type")?;

    let mut form = reqwest::multipart::Form::new()
        .text("model", whisper_model.to_string())
        .part("file", part);

    if let Some(lang) = language {
        form = form.text("language", lang.to_string());
    }

    let response = client
        .http
        .post(client.transcription_url())
        .bearer_auth(&client.api_key)
        .multipart(form)
        .send()
        .await
        .context("failed to send transcription request")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("whisper API error {status}: {body}");
    }

    let result: WhisperResponse = response
        .json()
        .await
        .context("failed to parse whisper response")?;

    Ok(join_broken_words(&result.text))
}

/// Whisper sometimes inserts a newline mid-word at a segment boundary, e.g.
/// "halb\nwegs" or "Bro\ncken". Rejoin any line pair where the first line ends
/// with a letter and the next line starts with a lowercase letter.
fn join_broken_words(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = String::with_capacity(text.len());
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            out.push_str(line);
            continue;
        }
        let prev_ends_with_letter = out.chars().next_back().map_or(false, |c| c.is_alphabetic());
        let next_starts_lowercase = line.chars().next().map_or(false, |c| c.is_lowercase());
        if prev_ends_with_letter && next_starts_lowercase {
            // mid-word break — join without separator
        } else {
            out.push('\n');
        }
        out.push_str(line);
    }
    out
}

/// Transcribe audio data, segmenting if necessary.
pub async fn transcribe(
    client: &OpenAiClient,
    whisper_model: &str,
    language: Option<&str>,
    file_path: &Path,
    filename: &str,
) -> Result<String> {
    let metadata = std::fs::metadata(file_path).context("failed to get file metadata")?;
    let needs_segmentation = metadata.len() > LONG_AUDIO_BYTES;

    if needs_segmentation {
        let segments = split_audio_into_segments(file_path)?;
        let mut parts: Vec<String> = Vec::new();
        for seg in segments {
            let text =
                transcribe_file(client, whisper_model, language, seg.path(), "segment.wav").await?;
            parts.push(text);
        }
        return Ok(parts.join(" "));
    }

    transcribe_file(client, whisper_model, language, file_path, filename).await
}
