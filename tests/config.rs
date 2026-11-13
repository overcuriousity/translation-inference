#![allow(clippy::unwrap_used, clippy::expect_used)]

use translation_inference::config::{PARSED_ENV_KEYS, load_from};

fn env(pairs: &[(&str, &str)]) -> impl Iterator<Item = (String, String)> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect::<Vec<_>>().into_iter()
}

#[test]
fn environment_variables_fill_nested_tables() {
    let cfg = load_from(
        None,
        env(&[
            ("TRANSLATION_INFERENCE__APP__BACKEND__URL", "http://127.0.0.1:11434"),
            ("TRANSLATION_INFERENCE__APP__BACKEND__MODEL", "qwen3"),
            ("TRANSLATION_INFERENCE__APP__BACKEND__MODELS", "[\"a\", \"b\"]"),
            ("TRANSLATION_INFERENCE__APP__FREE_CHAR_LIMIT", "0"),
            ("TRANSLATION_INFERENCE__APP__ACCESS_KEY", "000123456789"),
        ]),
    )
    .unwrap();
    let b = cfg.app.backend.as_ref().unwrap();
    assert_eq!(b.models, ["a", "b"]);
    assert_eq!(cfg.app.free_char_limit, 0);
    assert_eq!(cfg.app.access_key.as_deref(), Some("000123456789"), "a key of digits stays text");
    cfg.validate().unwrap();
}

#[test]
fn a_secret_can_come_from_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("key");
    std::fs::write(&path, "from-the-file-123\n").unwrap();
    let cfg = load_from(None, env(&[("TRANSLATION_INFERENCE__APP__ACCESS_KEY_FILE", path.to_str().unwrap())])).unwrap();
    assert_eq!(cfg.app.access_key.as_deref(), Some("from-the-file-123"));
    let url = format!("file://{}", path.display());
    let cfg = load_from(None, env(&[("TRANSLATION_INFERENCE__APP__ACCESS_KEY", &url)])).unwrap();
    assert_eq!(cfg.app.access_key.as_deref(), Some("from-the-file-123"));
}

#[test]
fn a_server_needs_a_backend_or_visitors_own() {
    let cfg = load_from(None, env(&[])).unwrap();
    assert!(cfg.validate().unwrap_err().to_string().contains("app.backend"));
}

#[test]
fn a_public_bind_needs_a_public_url_and_https() {
    let base = [("TRANSLATION_INFERENCE__APP__BYOK", "true"), ("TRANSLATION_INFERENCE__BIND", "0.0.0.0:8080")];
    let cfg = load_from(None, env(&base)).unwrap();
    assert!(cfg.validate().is_err());
    let mut with_http = base.to_vec();
    with_http.push(("TRANSLATION_INFERENCE__PUBLIC_URL", "http://translate.lan"));
    assert!(load_from(None, env(&with_http)).unwrap().validate().is_err());
    with_http.push(("TRANSLATION_INFERENCE__ALLOW_PLAIN_HTTP", "true"));
    load_from(None, env(&with_http)).unwrap().validate().unwrap();
}

#[test]
fn unknown_keys_are_refused() {
    assert!(load_from(None, env(&[("TRANSLATION_INFERENCE__APP__TYPO", "1")])).is_err());
}

#[test]
fn every_parsed_key_is_a_real_key() {
    let example = std::fs::read_to_string("config.example.toml").unwrap();
    for key in PARSED_ENV_KEYS {
        let leaf = key.rsplit('.').next().unwrap();
        assert!(example.contains(leaf), "{key} is not documented in config.example.toml");
    }
}

#[test]
fn the_example_config_is_valid() {
    let cfg = load_from(Some(std::path::Path::new("config.example.toml")), env(&[])).unwrap();
    cfg.validate().unwrap();
}
