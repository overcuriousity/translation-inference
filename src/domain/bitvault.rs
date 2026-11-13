//! Saving text as a BitVault paste, and opening a paste's text from its address.

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use crate::config::Bitvault;
use crate::error::Error;

/// BitVault caps a paste's text at 1 MiB.
pub const MAX_TEXT: usize = 1 << 20;

#[derive(Deserialize)]
struct Created {
    url: String,
}

/// Creates an unlisted paste and returns its address.
pub async fn save(http: &Client, bv: &Bitvault, text: &str) -> Result<String, Error> {
    if text.trim().is_empty() {
        return Err(Error::invalid("There is no text to save."));
    }
    if text.len() > MAX_TEXT {
        return Err(Error::invalid("BitVault keeps at most 1 MiB of text. Shorten it and try again."));
    }
    let endpoint = bv.url.join("api/v1/pastes").map_err(Error::internal)?;
    let mut req = http
        .post(endpoint)
        .timeout(Duration::from_secs(30))
        .json(&serde_json::json!({ "content": text, "privacy": "unlisted" }));
    if let Some(pw) = &bv.uploader_password {
        req = req.header("x-uploader-password", pw);
    }
    let res = super::upstream::check(req.send().await, "BitVault").await?;
    let created: Created = res.json().await.map_err(|e| Error::upstream("BitVault's answer could not be read.", e))?;
    Ok(created.url)
}

/// The raw address of the paste `link` names, when it is on the configured instance:
/// `https://paste.example/apple-banana-cherry` and its `/raw` both give the `/raw` address.
pub fn raw_url(bv: &Bitvault, link: &str) -> Option<Url> {
    let u = Url::parse(link.trim()).ok()?;
    let same = u.scheme() == bv.url.scheme()
        && u.host_str() == bv.url.host_str()
        && u.port_or_known_default() == bv.url.port_or_known_default();
    if !same || u.query().is_some_and(|q| !q.is_empty()) {
        return None;
    }
    let prefix = bv.url.path().trim_end_matches('/');
    let rest = u.path().strip_prefix(prefix)?.trim_matches('/');
    let words = rest.strip_suffix("/raw").unwrap_or(rest);
    // A paste address is three lower-case words joined by hyphens.
    let parts: Vec<&str> = words.split('-').collect();
    let plausible = words.len() <= 64
        && parts.len() == 3
        && parts.iter().all(|w| !w.is_empty() && w.bytes().all(|b| b.is_ascii_lowercase()));
    plausible.then(|| bv.url.join(&format!("{words}/raw")).ok()).flatten()
}

/// Fetches a paste's text, at most [`MAX_TEXT`] bytes.
pub async fn fetch(http: &Client, bv: &Bitvault, link: &str) -> Result<String, Error> {
    let url = raw_url(bv, link).ok_or_else(|| Error::invalid("That link is not a paste on this server's BitVault."))?;
    let res = super::upstream::check(http.get(url).timeout(Duration::from_secs(30)).send().await, "BitVault").await?;
    let mut body = res.bytes_stream();
    let mut buf = Vec::new();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| Error::upstream("The paste could not be read.", e))?;
        if buf.len() + chunk.len() > MAX_TEXT {
            return Err(Error::invalid("That paste is larger than 1 MiB."));
        }
        buf.extend_from_slice(&chunk);
    }
    String::from_utf8(buf).map_err(|_| Error::invalid("That paste is not text."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bv(url: &str) -> Bitvault {
        Bitvault { url: Url::parse(url).unwrap_or_else(|_| unreachable!()), uploader_password: None }
    }

    #[test]
    fn a_paste_address_and_its_raw_address_give_the_raw_address() {
        let b = bv("https://paste.example/");
        let want = Some("https://paste.example/apple-banana-cherry/raw".to_owned());
        assert_eq!(raw_url(&b, "https://paste.example/apple-banana-cherry").map(String::from), want);
        assert_eq!(raw_url(&b, "https://paste.example/apple-banana-cherry/raw").map(String::from), want);
    }

    #[test]
    fn links_elsewhere_are_refused() {
        let b = bv("https://paste.example/");
        assert!(raw_url(&b, "https://evil.example/apple-banana-cherry").is_none());
        assert!(raw_url(&b, "http://paste.example/apple-banana-cherry").is_none());
        assert!(raw_url(&b, "https://paste.example/../admin").is_none());
        assert!(raw_url(&b, "https://paste.example/apple?x=1").is_none());
    }
}
