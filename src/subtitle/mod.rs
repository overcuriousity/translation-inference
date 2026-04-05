/// A single subtitle cue (one caption block).
#[derive(Debug, Clone)]
pub struct SubtitleCue {
    /// Raw lines before the text: for SRT this is `"<index>\n<timecode>"`,
    /// for VTT this is just `"<timecode>"`.
    pub header: String,
    /// Text lines of this cue (may be more than one line).
    pub lines: Vec<String>,
}

/// Parse an SRT subtitle file into cues.
pub fn parse_srt(input: &str) -> Vec<SubtitleCue> {
    let mut cues = Vec::new();
    let normalised = input.replace("\r\n", "\n");
    let blocks: Vec<&str> = normalised
        .trim()
        .split("\n\n")
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .collect();

    for block in blocks {
        let lines: Vec<&str> = block.lines().collect();
        if lines.len() < 2 {
            continue;
        }
        // First line: sequence number; second line: timecode.
        // Any remaining lines: text content.
        if !lines[1].contains("-->") {
            // Malformed block — skip.
            continue;
        }
        let header = format!("{}\n{}", lines[0].trim(), lines[1].trim());
        let text_lines: Vec<String> = lines[2..].iter().map(|l| l.trim().to_string()).collect();
        if text_lines.is_empty() {
            continue;
        }
        cues.push(SubtitleCue { header, lines: text_lines });
    }
    cues
}

/// Parse a WebVTT subtitle file into cues.
pub fn parse_vtt(input: &str) -> Vec<SubtitleCue> {
    let mut cues = Vec::new();
    let normalised = input.replace("\r\n", "\n");
    let blocks: Vec<&str> = normalised
        .trim()
        .split("\n\n")
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .collect();

    for block in blocks {
        // Skip the WEBVTT header block and NOTE/STYLE/REGION blocks.
        let first_line = block.lines().next().unwrap_or("").trim();
        if first_line.starts_with("WEBVTT")
            || first_line.starts_with("NOTE")
            || first_line.starts_with("STYLE")
            || first_line.starts_with("REGION")
        {
            continue;
        }

        let lines: Vec<&str> = block.lines().collect();
        if lines.is_empty() {
            continue;
        }

        // VTT blocks may start with an optional cue identifier (no "-->"),
        // followed by the timecode line (contains "-->").
        let timecode_idx = lines.iter().position(|l| l.contains("-->"));
        let timecode_idx = match timecode_idx {
            Some(i) => i,
            None => continue, // not a cue block
        };

        // Header: everything up to and including the timecode line.
        let header = lines[..=timecode_idx]
            .iter()
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join("\n");

        let text_lines: Vec<String> = lines[timecode_idx + 1..]
            .iter()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();

        if text_lines.is_empty() {
            continue;
        }
        cues.push(SubtitleCue { header, lines: text_lines });
    }
    cues
}

/// Render cues back to SRT format.
pub fn render_srt(cues: &[SubtitleCue]) -> String {
    cues.iter()
        .enumerate()
        .map(|(i, cue)| {
            // Re-number sequentially in case the original numbering was off.
            let lines = cue.lines.join("\n");
            let parts: Vec<&str> = cue.header.splitn(2, '\n').collect();
            let timecode = parts.get(1).copied().unwrap_or(parts[0]);
            format!("{}\n{}\n{}", i + 1, timecode, lines)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Render cues back to VTT format.
pub fn render_vtt(cues: &[SubtitleCue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for cue in cues {
        out.push_str(&cue.header);
        out.push('\n');
        out.push_str(&cue.lines.join("\n"));
        out.push_str("\n\n");
    }
    out.trim_end().to_string()
}
