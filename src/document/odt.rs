use anyhow::{Context, Result};
use regex::Regex;
use std::io::{Cursor, Read, Write};
use std::sync::OnceLock;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::api::client::OpenAiClient;
use crate::document::translate_paragraphs;

const ODT_MAIN: &str = "content.xml";

/// Translate all text paragraphs/headings in an ODT file.
pub async fn translate_odt(
    bytes: &[u8],
    client: &OpenAiClient,
    model: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .context("failed to open ODT (ZIP) archive")?;

    let xml = read_entry(&mut archive, ODT_MAIN)
        .context("content.xml not found in ODT")?;
    let xml_str = String::from_utf8(xml).context("content.xml is not valid UTF-8")?;

    let para_ranges = find_para_ranges(&xml_str);

    let texts: Vec<String> = para_ranges
        .iter()
        .map(|r| extract_para_text(&xml_str[r.clone()]))
        .collect();

    let non_empty_indices: Vec<usize> = texts.iter()
        .enumerate()
        .filter(|(_, t)| !t.trim().is_empty())
        .map(|(i, _)| i)
        .collect();

    if non_empty_indices.is_empty() {
        return repack_zip(bytes, ODT_MAIN, xml_str.as_bytes());
    }

    let non_empty_texts: Vec<&str> = non_empty_indices.iter()
        .map(|&i| texts[i].as_str())
        .collect();

    let translated_non_empty =
        translate_paragraphs(&non_empty_texts, client, model, source_lang, target_lang)
            .await
            .context("translation failed")?;

    let mut translated_texts: Vec<String> = texts.iter().map(|_| String::new()).collect();
    for (j, &orig_idx) in non_empty_indices.iter().enumerate() {
        translated_texts[orig_idx] = translated_non_empty[j].clone();
    }

    let mut new_xml = xml_str.clone();
    for (i, range) in para_ranges.iter().enumerate().rev() {
        let translation = &translated_texts[i];
        if !translation.is_empty() {
            let para_xml = &xml_str[range.clone()];
            let new_para = replace_para_content(para_xml, translation);
            new_xml.replace_range(range.clone(), &new_para);
        }
    }

    repack_zip(bytes, ODT_MAIN, new_xml.as_bytes())
}

/// Find byte ranges for each `<text:p>` or `<text:h>` element.
fn find_para_ranges(xml: &str) -> Vec<std::ops::Range<usize>> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(
        r"(?s)<text:(?:p|h)(?:>|\s[^>]*>).*?</text:(?:p|h)>|<text:(?:p|h)(?:\s[^>]*)?\s*/>"
    ).unwrap());
    re.find_iter(xml).map(|m| m.range()).collect()
}

/// Strip all XML tags to extract plain text from a paragraph.
fn extract_para_text(para_xml: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let tag_re = RE.get_or_init(|| Regex::new(r"<[^>]+>").unwrap());
    let text = tag_re.replace_all(para_xml, "");
    xml_unescape(text.trim())
}

/// Replace the inner content of a `<text:p>` or `<text:h>` element with
/// the translation, preserving inline `<text:span>` structure.
///
/// When spans are present the first span receives the full translation and
/// subsequent spans are emptied — matching what the DOCX path does for `<w:t>`
/// runs and ensuring that at least the first span's character style is kept.
/// When there are no spans the paragraph's own text node is replaced directly.
fn replace_para_content(para_xml: &str, translation: &str) -> String {
    static SPAN_RE: OnceLock<Regex> = OnceLock::new();
    let span_re = SPAN_RE.get_or_init(|| {
        Regex::new(r"(?s)<text:span(?:>|\s[^>]*>).*?</text:span>").unwrap()
    });

    let escaped = xml_escape(translation);

    if span_re.is_match(para_xml) {
        let mut first = true;
        return span_re.replace_all(para_xml, |caps: &regex::Captures| {
            let span = &caps[0];
            // Keep the opening tag (with its style attributes) intact.
            let open_end = span.find('>').map(|i| i + 1).unwrap_or(span.len());
            let open_tag = &span[..open_end];
            if first {
                first = false;
                format!("{}{}</text:span>", open_tag, escaped)
            } else {
                // Empty subsequent spans; their run-level style is preserved in
                // case the document is later edited.
                format!("{}</text:span>", open_tag)
            }
        }).to_string();
    }

    // No spans — replace the text content of the paragraph element directly.
    let close_tag = if para_xml.contains("</text:p>") {
        "</text:p>"
    } else if para_xml.contains("</text:h>") {
        "</text:h>"
    } else {
        return para_xml.to_string();
    };

    let open_end = match para_xml.find('>') {
        Some(i) => i + 1,
        None => return para_xml.to_string(),
    };

    format!("{}{}{}", &para_xml[..open_end], escaped, close_tag)
}

/// Extract plain text paragraphs from an ODT file, including empty ones to preserve layout.
pub fn extract_odt_paragraphs(bytes: &[u8]) -> Result<Vec<String>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .context("failed to open ODT (ZIP) archive")?;
    let xml = read_entry(&mut archive, ODT_MAIN)
        .context("content.xml not found in ODT")?;
    let xml_str = String::from_utf8(xml).context("content.xml is not valid UTF-8")?;
    let para_ranges = find_para_ranges(&xml_str);
    let texts: Vec<String> = para_ranges
        .iter()
        .map(|r| extract_para_text(&xml_str[r.clone()]))
        .collect();
    Ok(texts)
}

/// Create a minimal ODT ZIP from a list of paragraph strings.
pub fn build_odt_from_paragraphs(paragraphs: &[String]) -> Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);

    // mimetype must be stored (uncompressed) and first
    let stored_opts = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);
    zip.start_file("mimetype", stored_opts)?;
    zip.write_all(b"application/vnd.oasis.opendocument.text")?;

    let deflate_opts = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // META-INF/manifest.xml
    let manifest = r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">
  <manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/>
  <manifest:file-entry manifest:full-path="mimetype" manifest:media-type="text/plain"/>
  <manifest:file-entry manifest:full-path="META-INF/manifest.xml" manifest:media-type="text/xml"/>
  <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
</manifest:manifest>"#;
    zip.start_file("META-INF/manifest.xml", deflate_opts)?;
    zip.write_all(manifest.as_bytes())?;

    // content.xml
    let mut content = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <office:document-content \
           xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" \
           xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" \
           office:version=\"1.3\">\
         <office:automatic-styles/>\
         <office:body><office:text>",
    );
    for para in paragraphs {
        content.push_str("<text:p text:style-name=\"Standard\">");
        content.push_str(&xml_escape(para));
        content.push_str("</text:p>");
    }
    content.push_str("</office:text></office:body></office:document-content>");
    zip.start_file("content.xml", deflate_opts)?;
    zip.write_all(content.as_bytes())?;

    Ok(zip.finish()?.into_inner())
}

/// Create a minimal ODT from plain text, splitting on double newlines (or single newlines).
pub fn build_odt_from_text(text: &str) -> Result<Vec<u8>> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let paragraphs: Vec<String> = if normalized.contains("\n\n") {
        // When there are double newlines, treat them as paragraph separators, but
        // preserve additional newlines in longer runs as empty paragraphs instead of
        // collapsing them into leading newlines of the next paragraph.
        let mut paragraphs = Vec::new();
        let mut current = String::new();
        let chars: Vec<char> = normalized.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] != '\n' {
                current.push(chars[i]);
                i += 1;
            } else {
                // We have at least one '\n'; count how many consecutive newlines.
                let mut j = i;
                while j < chars.len() && chars[j] == '\n' {
                    j += 1;
                }
                let run = j - i;
                if run == 1 {
                    // Single newline inside a paragraph: keep as a literal newline.
                    current.push('\n');
                } else {
                    // One or more paragraph separators: each pair of newlines ends
                    // the current paragraph and starts a new one (possibly empty).
                    let pairs = run / 2;
                    for _ in 0..pairs {
                        paragraphs.push(current);
                        current = String::new();
                    }
                    if run % 2 == 1 {
                        // Leftover newline from an odd-length run: treat it as an
                        // additional separator that creates an empty paragraph,
                        // instead of leaving it as a leading newline in the next
                        // paragraph.
                        paragraphs.push(current.clone());
                        current.clear();
                    }
                }
                i = j;
            }
        }
        paragraphs.push(current);
        paragraphs
    } else {
        normalized.split('\n').map(|s| s.to_string()).collect()
    };
    build_odt_from_paragraphs(&paragraphs)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn xml_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .to_string()
}

fn read_entry(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>> {
    let mut file = archive.by_name(name)
        .with_context(|| format!("entry '{name}' not found in archive"))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

fn repack_zip(original_bytes: &[u8], replace_name: &str, new_content: &[u8]) -> Result<Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(original_bytes))
        .context("failed to re-open archive for repacking")?;

    let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        entries.push((name, data));
    }

    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let opts = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for (name, data) in &entries {
        zip.start_file(name, opts)?;
        if name == replace_name {
            zip.write_all(new_content)?;
        } else {
            zip.write_all(data)?;
        }
    }

    Ok(zip.finish()?.into_inner())
}
