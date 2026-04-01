use anyhow::{Context, Result};
use regex::Regex;
use std::io::{Cursor, Read, Write};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::api::{chat, client::OpenAiClient};

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

    // Collect non-empty paragraph texts for translation
    let non_empty: Vec<&str> = texts.iter()
        .filter(|t| !t.trim().is_empty())
        .map(|t| t.as_str())
        .collect();

    if non_empty.is_empty() {
        return repack_zip(bytes, DOCX_MAIN, xml_str.as_bytes());
    }

    let full_text = non_empty.join("\n\n---PARA-SEP---\n\n");
    let (translated, _, _) = chat::translate(client, model, source_lang, target_lang, &full_text).await
        .context("translation failed")?;

    // Split translated text back into paragraph-sized pieces
    let translated_parts: Vec<&str> = translated.split("\n\n---PARA-SEP---\n\n").collect();
    let mut trans_iter = translated_parts.iter();

    let translated_texts: Vec<String> = texts.iter().map(|t| {
        if t.trim().is_empty() {
            String::new()
        } else {
            trans_iter.next().copied().unwrap_or("").to_string()
        }
    }).collect();

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
    // Match <w:p> or <w:p ...> ... </w:p>, and self-closing <w:p/>
    // The (?:>|\s[^>]*>) ensures we don't match <w:pPr> etc.
    let re = Regex::new(r"(?s)<w:p(?:>|\s[^>]*>).*?</w:p>|<w:p(?:\s[^>]*)?\s*/>").unwrap();
    re.find_iter(xml).map(|m| m.range()).collect()
}

/// Extract all text content from a paragraph's XML.
fn extract_para_text(para_xml: &str) -> String {
    let re = Regex::new(r"(?s)<w:t(?:>|\s[^>]*>)(.*?)</w:t>").unwrap();
    re.captures_iter(para_xml)
        .map(|c| xml_unescape(&c[1]))
        .collect::<Vec<_>>()
        .join("")
}

/// Replace the content of all `<w:t>` elements in a paragraph:
/// the first one gets the translated text, the rest are emptied.
fn replace_para_text(para_xml: &str, translation: &str) -> String {
    let re = Regex::new(r"(?s)<w:t(?:>|\s[^>]*>).*?</w:t>").unwrap();
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
