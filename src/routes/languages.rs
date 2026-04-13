use axum::Json;

use crate::models::{Language, LanguagesResponse};

/// Canonical list of supported target language codes (excluding "auto").
/// Used both to populate the `/api/languages` response and to validate
/// `target_lang` in translation requests.
pub const VALID_TARGET_LANGS: &[(&str, &str)] = &[
    ("en", "English"),
    ("de", "German"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
    ("nl", "Dutch"),
    ("pl", "Polish"),
    ("ru", "Russian"),
    ("zh", "Chinese (Simplified)"),
    ("zh-TW", "Chinese (Traditional)"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("vi", "Vietnamese"),
    ("id", "Indonesian"),
    ("ar", "Arabic"),
    ("tr", "Turkish"),
    ("uk", "Ukrainian"),
    ("cs", "Czech"),
    ("sv", "Swedish"),
    ("da", "Danish"),
    ("fi", "Finnish"),
    ("no", "Norwegian"),
    ("ro", "Romanian"),
    ("hu", "Hungarian"),
    ("bg", "Bulgarian"),
    ("el", "Greek"),
    ("hi", "Hindi"),
];

pub async fn get_languages() -> Json<LanguagesResponse> {
    let mut languages = vec![Language {
        code: "auto".into(),
        name: "Auto-detect".into(),
    }];
    for &(code, name) in VALID_TARGET_LANGS {
        languages.push(Language {
            code: code.into(),
            name: name.into(),
        });
    }
    Json(LanguagesResponse { languages })
}
