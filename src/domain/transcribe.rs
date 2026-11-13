//! Speech to text: ffmpeg turns any audio or video into 16 kHz mono WAV, long recordings are cut
//! into ten-minute segments, and each goes to `/v1/audio/transcriptions`.

use std::path::Path;

use reqwest::Client;
use serde::Deserialize;
use tempfile::{NamedTempFile, TempDir};
use tokio::process::Command;

use super::upstream::{Endpoint, check};
use crate::error::Error;

/// Above this a recording is cut into segments, which keeps each request below the 25 MB that
/// hosted endpoints accept.
const SEGMENT_ABOVE: u64 = 24 * 1024 * 1024;

const MEDIA: &[&str] = &[
    "mp3", "wav", "m4a", "ogg", "oga", "flac", "aac", "wma", "aiff", "opus", "webm", "mp4", "mkv", "avi", "mov", "flv",
    "wmv", "m4v", "3gp", "ts", "mpeg", "mpg", "vob", "mts", "m2ts",
];

pub fn is_media(filename: &str) -> bool {
    let ext = Path::new(filename).extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    MEDIA.contains(&ext.as_str())
}

pub async fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").output().await.is_ok_and(|o| o.status.success())
}

async fn ffmpeg(args: &[&std::ffi::OsStr]) -> Result<(), Error> {
    let out = Command::new("ffmpeg")
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .output()
        .await
        .map_err(|e| Error::internal(format!("ffmpeg could not run: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::Invalid("That file has no audio that could be read. Choose a different file.".into()))
            .inspect_err(|_| tracing::warn!(stderr = %String::from_utf8_lossy(&out.stderr), "ffmpeg failed"))
    }
}

/// Converts the upload to WAV, then cuts it when it is long.
async fn prepare(input: &Path) -> Result<(TempDir, Vec<std::path::PathBuf>), Error> {
    let dir = tempfile::tempdir()?;
    let wav = dir.path().join("audio.wav");
    let pcm = ["-vn", "-acodec", "pcm_s16le", "-ar", "16000", "-ac", "1"].map(std::ffi::OsStr::new);
    let mut args = vec![std::ffi::OsStr::new("-i"), input.as_os_str()];
    args.extend(pcm);
    args.push(wav.as_os_str());
    ffmpeg(&args).await?;
    if tokio::fs::metadata(&wav).await?.len() <= SEGMENT_ABOVE {
        return Ok((dir, vec![wav]));
    }
    let pattern = dir.path().join("segment_%03d.wav");
    let seg = ["-f", "segment", "-segment_time", "600", "-c", "copy", "-reset_timestamps", "1"].map(std::ffi::OsStr::new);
    let mut args = vec![std::ffi::OsStr::new("-i"), wav.as_os_str()];
    args.extend(seg);
    args.push(pattern.as_os_str());
    ffmpeg(&args).await?;
    let mut segments = Vec::new();
    for i in 0.. {
        let p = dir.path().join(format!("segment_{i:03}.wav"));
        if !p.exists() {
            break;
        }
        segments.push(p);
    }
    Ok((dir, segments))
}

#[derive(Deserialize)]
struct Transcript {
    text: String,
}

async fn send(http: &Client, ep: &Endpoint, model: &str, language: Option<&str>, wav: &Path) -> Result<String, Error> {
    let file = tokio::fs::File::open(wav).await?;
    let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(file));
    let part = reqwest::multipart::Part::stream(body).file_name("audio.wav").mime_str("audio/wav").map_err(Error::internal)?;
    let mut form = reqwest::multipart::Form::new().text("model", model.to_owned()).part("file", part);
    if let Some(lang) = language {
        form = form.text("language", lang.to_owned());
    }
    let res = check(ep.post(http, "/v1/audio/transcriptions").multipart(form).send().await, "transcription").await?;
    let t: Transcript = res.json().await.map_err(|e| Error::upstream("The transcript could not be read.", e))?;
    Ok(join_broken_words(&t.text))
}

/// Transcribes the audio or video in `input`. `language` is a two-letter hint, or `None` to let
/// the model detect it.
pub async fn transcribe(http: &Client, ep: &Endpoint, model: &str, language: Option<&str>, input: &NamedTempFile) -> Result<String, Error> {
    let (_dir, segments) = prepare(input.path()).await?;
    let mut parts = Vec::with_capacity(segments.len());
    for seg in &segments {
        parts.push(send(http, ep, model, language, seg).await?);
    }
    Ok(parts.join(" ").trim().to_owned())
}

/// Whisper sometimes breaks a line inside a word at a segment boundary ("halb\nwegs"): a line
/// ending in a letter followed by one starting in lower case is joined.
pub fn join_broken_words(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        let joins = out.chars().next_back().is_some_and(char::is_alphabetic) && line.chars().next().is_some_and(char::is_lowercase);
        if i > 0 && !joins {
            out.push('\n');
        }
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_broken_across_lines_is_joined() {
        assert_eq!(join_broken_words("halb\nwegs gut\nNeu"), "halbwegs gut\nNeu");
    }

    #[test]
    fn media_files_are_recognised_by_extension() {
        assert!(is_media("talk.MKV"));
        assert!(!is_media("notes.txt"));
    }
}
