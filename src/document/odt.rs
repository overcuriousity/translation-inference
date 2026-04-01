use anyhow::{Context, Result};
use regex::Regex;
use std::io::{Cursor, Read, Write};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::api::{chat, client::OpenAiClient};

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

    let non_empty: Vec<&str> = texts.iter()
        .filter(|t| !t.trim().is_empty())
        .map(|t| t.as_str())
        .collect();

    if non_empty.is_empty() {
        return repack_zip(bytes, ODT_MAIN, xml_str.as_bytes());
    }

    let full_text = non_empty.join("\n\n---PARA-SEP---\n\n");
    let (translated, _, _) = chat::translate(client, model, source_lang, target_lang, &full_text).await
        .context("translation failed")?;

    let translated_parts: Vec<&str> = translated.split("\n\n---PARA-SEP---\n\n").collect();
    let mut trans_iter = translated_parts.iter();

    let translated_texts: Vec<String> = texts.iter().map(|t| {
        if t.trim().is_empty() {
            String::new()
        } else {
            trans_iter.next().copied().unwrap_or("").to_string()
        }
    }).collect();

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
    let re = Regex::new(
        r"(?s)<text:(?:p|h)(?:>|\s[^>]*>).*?</text:(?:p|h)>|<text:(?:p|h)(?:\s[^>]*)?\s*/>"
    ).unwrap();
    re.find_iter(xml).map(|m| m.range()).collect()
}

/// Strip all XML tags to extract plain text from a paragraph.
fn extract_para_text(para_xml: &str) -> String {
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    let text = tag_re.replace_all(para_xml, "");
    xml_unescape(text.trim())
}

/// Replace the inner content of a `<text:p>` or `<text:h>` element.
fn replace_para_content(para_xml: &str, translation: &str) -> String {
    let escaped = xml_escape(translation);

    // Determine the close tag
    let close_tag = if para_xml.contains("</text:p>") {
        "</text:p>"
    } else if para_xml.contains("</text:h>") {
        "</text:h>"
    } else {
        return para_xml.to_string();
    };

    // Find end of opening tag
    let open_end = match para_xml.find('>') {
        Some(i) => i + 1,
        None => return para_xml.to_string(),
    };

    format!("{}{}{}", &para_xml[..open_end], escaped, close_tag)
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
