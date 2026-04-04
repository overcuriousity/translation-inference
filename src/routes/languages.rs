use axum::Json;

use crate::models::{Language, LanguagesResponse};

pub async fn get_languages() -> Json<LanguagesResponse> {
    let languages = vec![
        Language { code: "auto".into(), name: "Auto-detect".into() },
        Language { code: "en".into(), name: "English".into() },
        Language { code: "de".into(), name: "German".into() },
        Language { code: "fr".into(), name: "French".into() },
        Language { code: "es".into(), name: "Spanish".into() },
        Language { code: "it".into(), name: "Italian".into() },
        Language { code: "pt".into(), name: "Portuguese".into() },
        Language { code: "nl".into(), name: "Dutch".into() },
        Language { code: "pl".into(), name: "Polish".into() },
        Language { code: "ru".into(), name: "Russian".into() },
        Language { code: "zh".into(), name: "Chinese (Simplified)".into() },
        Language { code: "zh-TW".into(), name: "Chinese (Traditional)".into() },
        Language { code: "ja".into(), name: "Japanese".into() },
        Language { code: "ko".into(), name: "Korean".into() },
        Language { code: "vi".into(), name: "Vietnamese".into() },
        Language { code: "id".into(), name: "Indonesian".into() },
        Language { code: "ar".into(), name: "Arabic".into() },
        Language { code: "tr".into(), name: "Turkish".into() },
        Language { code: "uk".into(), name: "Ukrainian".into() },
        Language { code: "cs".into(), name: "Czech".into() },
        Language { code: "sv".into(), name: "Swedish".into() },
        Language { code: "da".into(), name: "Danish".into() },
        Language { code: "fi".into(), name: "Finnish".into() },
        Language { code: "no".into(), name: "Norwegian".into() },
        Language { code: "ro".into(), name: "Romanian".into() },
        Language { code: "hu".into(), name: "Hungarian".into() },
        Language { code: "bg".into(), name: "Bulgarian".into() },
        Language { code: "el".into(), name: "Greek".into() },
        Language { code: "hi".into(), name: "Hindi".into() },
    ];

    Json(LanguagesResponse { languages })
}
