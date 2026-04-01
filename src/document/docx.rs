use anyhow::{Context, Result};
use regex::Regex;
use std::io::{Cursor, Read, Write};
use std::sync::OnceLock;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::api::client::OpenAiClient;
use crate::document::translate_paragraphs;

const DOCX_MAIN: &str = "word/document.xml";

/// Translate all text paragraphs in a DOCX file, returning a new DOCX as bytes.
pub async fn translate_docx(
    bytes: &[u8],
    client: &OpenAiClient,
    model: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .context("failed to open DOCX (ZIP) archive")?;

    let xml = read_entry(&mut archive, DOCX_MAIN)
        .context("word/document.xml not found in DOCX")?;
    let xml_str = String::from_utf8(xml).context("word/document.xml is not valid UTF-8")?;

    // Find all paragraph ranges (byte offsets into xml_str)
    let para_ranges = find_para_ranges(&xml_str);

    // Extract text per paragraph
    let texts: Vec<String> = para_ranges
        .iter()
        .map(|r| extract_para_text(&xml_str[r.clone()]))
        .collect();

    // Collect indices and refs of non-empty paragraphs
    let non_empty_indices: Vec<usize> = texts.iter()
        .enumerate()
        .filter(|(_, t)| !t.trim().is_empty())
        .map(|(i, _)| i)
        .collect();

    if non_empty_indices.is_empty() {
        return repack_zip(bytes, DOCX_MAIN, xml_str.as_bytes());
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

    // Reconstruct XML, replacing paragraph content in reverse order
    let mut new_xml = xml_str.clone();
    for (i, range) in para_ranges.iter().enumerate().rev() {
        let translation = &translated_texts[i];
        if !translation.is_empty() {
            let para_xml = &xml_str[range.clone()];
            let new_para = replace_para_text(para_xml, translation);
            new_xml.replace_range(range.clone(), &new_para);
        }
    }

    repack_zip(bytes, DOCX_MAIN, new_xml.as_bytes())
}

/// Find byte ranges for each `<w:p>...</w:p>` block in the XML.
fn find_para_ranges(xml: &str) -> Vec<std::ops::Range<usize>> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // The (?:>|\s[^>]*>) ensures we don't match <w:pPr> etc.
        Regex::new(r"(?s)<w:p(?:>|\s[^>]*>).*?</w:p>|<w:p(?:\s[^>]*)?\s*/>").unwrap()
    });
    re.find_iter(xml).map(|m| m.range()).collect()
}

/// Extract all text content from a paragraph's XML.
fn extract_para_text(para_xml: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?s)<w:t(?:>|\s[^>]*>)(.*?)</w:t>").unwrap());
    re.captures_iter(para_xml)
        .map(|c| xml_unescape(&c[1]))
        .collect::<Vec<_>>()
        .join("")
}

/// Replace the content of all `<w:t>` elements in a paragraph:
/// the first one gets the translated text, the rest are emptied.
fn replace_para_text(para_xml: &str, translation: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?s)<w:t(?:>|\s[^>]*>).*?</w:t>").unwrap());
    let escaped = xml_escape(translation);
    let mut first = true;
    re.replace_all(para_xml, |_: &regex::Captures| {
        if first {
            first = false;
            format!(r#"<w:t xml:space="preserve">{escaped}</w:t>"#)
        } else {
            "<w:t/>".to_string()
        }
    })
    .to_string()
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

    // Collect all entries first (borrow ends before we write)
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
