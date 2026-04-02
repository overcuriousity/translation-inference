use anyhow::{Context, Result};
use regex::Regex;
use std::io::{Cursor, Read};
use std::sync::OnceLock;
use zip::ZipArchive;

const DOCX_MAIN: &str = "word/document.xml";

/// Extract all non-empty paragraph texts from a DOCX file.
pub fn extract_docx_paragraphs(bytes: &[u8]) -> Result<Vec<String>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .context("failed to open DOCX (ZIP) archive")?;

    let xml = read_entry(&mut archive, DOCX_MAIN)
        .context("word/document.xml not found in DOCX")?;
    let xml_str = String::from_utf8(xml).context("word/document.xml is not valid UTF-8")?;

    let para_ranges = find_para_ranges(&xml_str);
    let texts: Vec<String> = para_ranges
        .iter()
        .map(|r| extract_para_text(&xml_str[r.clone()]))
        .filter(|t| !t.trim().is_empty())
        .collect();

    Ok(texts)
}

/// Find byte ranges for each `<w:p>...</w:p>` block in the XML.
fn find_para_ranges(xml: &str) -> Vec<std::ops::Range<usize>> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
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
