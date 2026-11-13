use askama::Template;
use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use super::{Extras, IconSize, Shell, icon, page_with, render};
use crate::assets::Assets;
use crate::domain::languages::LANGUAGES;
use crate::domain::{bitvault, markdown};
use crate::error::Error;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(text))
        .route("/files", get(files))
        .route("/talk", get(talk))
        .route("/markdown", post(render_markdown))
}

/// What the page scripts need to know, rendered into `data-` attributes.
struct Features {
    char_limit: Option<usize>,
    /// Languages with a voice, space-separated.
    speech: String,
    transcribe: bool,
    bitvault: bool,
}

impl Features {
    fn of(st: &AppState, shell: &Shell) -> Self {
        let a = &shell.access;
        Self {
            char_limit: a.char_limit(st),
            speech: st.cfg.app.speech.as_ref().map(|s| s.voices.keys().cloned().collect::<Vec<_>>().join(" ")).unwrap_or_default(),
            transcribe: a.transcription(st).is_ok() && a.transcription_model(st, None).is_ok(),
            bitvault: st.cfg.app.bitvault.is_some(),
        }
    }
}

fn tool(name: &str) -> String {
    icon(name, IconSize::Tool)
}

#[derive(Template)]
#[template(path = "text.html")]
struct TextPage {
    languages: &'static [(&'static str, &'static str)],
    f: Features,
    text: String,
    /// Why `?from=` could not be opened.
    from_error: Option<String>,
    notice_icon: String,
    copy_icons: String,
}

impl TextPage {
    fn icon(&self, name: &str) -> String {
        tool(name)
    }
}

#[derive(Deserialize)]
struct TextQuery {
    from: Option<String>,
}

async fn text(State(st): State<AppState>, shell: Shell, Query(q): Query<TextQuery>) -> Result<Html<String>, Error> {
    let (text, from_error) = match (q.from.as_deref().filter(|f| !f.trim().is_empty()), &st.cfg.app.bitvault) {
        (Some(link), Some(bv)) => match bitvault::fetch(&st.http, bv, link).await {
            Ok(t) => (t, None),
            Err(e) => {
                e.log();
                (String::new(), Some(e.to_string()))
            }
        },
        (Some(_), None) => (String::new(), Some("This server has no BitVault to open the link from.".to_owned())),
        (None, _) => (String::new(), None),
    };
    let page = TextPage {
        languages: LANGUAGES,
        f: Features::of(&st, &shell),
        text,
        from_error,
        notice_icon: super::notice_icon("danger"),
        copy_icons: super::copy_icons(),
    };
    let extras = Extras { js: vec![Assets::url("js/text.js")], ..Extras::default() };
    page_with(&st, &shell, "text", "Translate", render(&page)?, extras)
}

#[derive(Template)]
#[template(path = "files.html")]
struct FilesPage {
    languages: &'static [(&'static str, &'static str)],
}

impl FilesPage {
    fn icon(&self, name: &str) -> String {
        tool(name)
    }
}

async fn files(State(st): State<AppState>, shell: Shell) -> Result<Html<String>, Error> {
    if !shell.access.files() {
        return Err(Error::Forbidden);
    }
    let extras = Extras { js: vec![Assets::url("js/files.js")], ..Extras::default() };
    page_with(&st, &shell, "files", "Files", render(&FilesPage { languages: LANGUAGES })?, extras)
}

#[derive(Template)]
#[template(path = "talk.html")]
struct TalkPage {
    languages: &'static [(&'static str, &'static str)],
    f: Features,
}

impl TalkPage {
    fn icon(&self, name: &str) -> String {
        tool(name)
    }
}

async fn talk(State(st): State<AppState>, shell: Shell) -> Result<Html<String>, Error> {
    let page = TalkPage { languages: LANGUAGES, f: Features::of(&st, &shell) };
    let extras = Extras { js: vec![Assets::url("js/talk.js")], ..Extras::default() };
    page_with(&st, &shell, "talk", "Talk", render(&page)?, extras)
}

#[derive(Deserialize)]
struct MarkdownRequest {
    text: String,
}

#[derive(serde::Serialize)]
struct MarkdownResponse {
    html: String,
}

/// The output's Markdown as HTML, for the Markdown view.
async fn render_markdown(Json(req): Json<MarkdownRequest>) -> Json<MarkdownResponse> {
    Json(MarkdownResponse { html: markdown::render(&req.text) })
}
