use std::sync::Arc;

use askama::Template;

use crate::assets::Assets;
use crate::config::Config;

pub struct NavItem {
    pub href: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub current: bool,
    /// Shown in the top bar only: the tab bar keeps to a few destinations.
    pub wide_only: bool,
}

/// The page frame: top bar, main region, bottom tab bar and the notices region.
#[derive(Template)]
#[template(path = "layout.html")]
pub struct Layout {
    pub title: String,
    pub current: &'static str,
    pub body: String,
    /// The visitor may translate files, so the Files tab is shown.
    pub files: bool,
    pub extra_css: Vec<String>,
    /// ES modules.
    pub extra_js: Vec<String>,
    pub cfg: Arc<Config>,
}

pub const KIT_CSS: [&str; 6] = [
    "vendor/oc/css/00-tokens.css",
    "vendor/oc/css/10-base.css",
    "vendor/oc/css/20-layout.css",
    "vendor/oc/css/30-components.css",
    "vendor/oc/css/50-touch.css",
    "vendor/oc/css/profile-everyday.css",
];

pub const MODULES: [&str; 6] = [
    "js/prefs.js",
    "vendor/oc/js/copy.js",
    "vendor/oc/js/notices.js",
    "vendor/oc/js/menu.js",
    "vendor/oc/js/keys.js",
    "vendor/oc/js/busy.js",
];

impl Layout {
    fn asset(&self, path: &str) -> String {
        Assets::url(path)
    }

    fn stylesheets(&self) -> Vec<String> {
        let own = [Assets::url("css/app.css")];
        KIT_CSS.iter().map(|p| Assets::url(p)).chain(own).chain(self.extra_css.iter().cloned()).collect()
    }

    fn modules(&self) -> Vec<String> {
        MODULES.iter().map(|p| Assets::url(p)).chain(self.extra_js.iter().cloned()).collect()
    }

    fn nav(&self) -> Vec<NavItem> {
        let item = |href, label, icon, key| NavItem { href, label, icon, current: self.current == key, wide_only: false };
        let mut items = vec![item("/", "Text", "languages", "text")];
        if self.files {
            items.push(item("/files", "Files", "file-text", "files"));
        }
        items.push(item("/talk", "Talk", "messages-square", "talk"));
        items.push(item("/settings", "Settings", "settings", "settings"));
        items.push(NavItem { wide_only: true, ..item("/docs", "API", "book-open", "docs") });
        items
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn icon(&self, name: &str) -> String {
        super::icon(name, super::IconSize::Tool)
    }

    fn tab_icon(&self, name: &str) -> String {
        super::icon(name, super::IconSize::Tab)
    }

    fn logo(&self) -> String {
        Assets::get("icon.svg").map(|f| String::from_utf8_lossy(&f.data).trim_end().to_owned()).unwrap_or_default()
    }
}
