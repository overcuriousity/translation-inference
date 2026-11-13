# Copied design files

Copied unmodified from overcuriousity/overcuriousity-dev-manifest, tag v2026.11.1. Updating means a deliberate re-copy from a newer tag.

| Path here (under assets/vendor/oc/) | Source path in the manifest repository |
|---|---|
| css/00-tokens.css | crates/oc-kit/assets/css/00-tokens.css |
| css/10-base.css | crates/oc-kit/assets/css/10-base.css |
| css/20-layout.css | crates/oc-kit/assets/css/20-layout.css |
| css/30-components.css | crates/oc-kit/assets/css/30-components.css |
| css/50-touch.css | crates/oc-kit/assets/css/50-touch.css |
| css/profile-everyday.css | crates/oc-kit/assets/css/profile-everyday.css |
| fonts/atkinson-next-400.woff2 | crates/oc-kit/assets/fonts/atkinson-next-400.woff2 |
| fonts/atkinson-next-500.woff2 | crates/oc-kit/assets/fonts/atkinson-next-500.woff2 |
| fonts/atkinson-next-600.woff2 | crates/oc-kit/assets/fonts/atkinson-next-600.woff2 |
| fonts/jetbrains-mono-400.woff2 | crates/oc-kit/assets/fonts/jetbrains-mono-400.woff2 |
| fonts/OFL-AtkinsonHyperlegibleNext.txt | crates/oc-kit/assets/fonts/OFL-AtkinsonHyperlegibleNext.txt |
| fonts/OFL-JetBrainsMono.txt | crates/oc-kit/assets/fonts/OFL-JetBrainsMono.txt |
| icons/a-large-small.svg | crates/oc-kit/assets/icons/a-large-small.svg |
| icons/arrow-left-right.svg | translation-inference addition from Lucide 1.72.0 (lucide-icons/lucide, icons/arrow-left-right.svg), ISC |
| icons/book-open.svg | crates/oc-kit/assets/icons/book-open.svg |
| icons/check.svg | crates/oc-kit/assets/icons/check.svg |
| icons/chevron-down.svg | crates/oc-kit/assets/icons/chevron-down.svg |
| icons/circle-check.svg | crates/oc-kit/assets/icons/circle-check.svg |
| icons/circle-x.svg | crates/oc-kit/assets/icons/circle-x.svg |
| icons/columns-2.svg | translation-inference addition from Lucide 1.72.0 (lucide-icons/lucide, icons/columns-2.svg), ISC |
| icons/copy.svg | crates/oc-kit/assets/icons/copy.svg |
| icons/download.svg | crates/oc-kit/assets/icons/download.svg |
| icons/file-text.svg | translation-inference addition from Lucide 1.72.0 (lucide-icons/lucide, icons/file-text.svg), ISC |
| icons/info.svg | crates/oc-kit/assets/icons/info.svg |
| icons/key-round.svg | crates/oc-kit/assets/icons/key-round.svg |
| icons/languages.svg | translation-inference addition from Lucide 1.72.0 (lucide-icons/lucide, icons/languages.svg), ISC |
| icons/LICENSE-lucide.txt | crates/oc-kit/assets/icons/LICENSE-lucide.txt |
| icons/link.svg | crates/oc-kit/assets/icons/link.svg |
| icons/messages-square.svg | translation-inference addition from Lucide 1.72.0 (lucide-icons/lucide, icons/messages-square.svg), ISC |
| icons/mic.svg | translation-inference addition from Lucide 1.72.0 (lucide-icons/lucide, icons/mic.svg), ISC |
| icons/monitor.svg | crates/oc-kit/assets/icons/monitor.svg |
| icons/moon.svg | crates/oc-kit/assets/icons/moon.svg |
| icons/play.svg | translation-inference addition from Lucide 1.72.0 (lucide-icons/lucide, icons/play.svg), ISC |
| icons/save.svg | translation-inference addition from Lucide 1.72.0 (lucide-icons/lucide, icons/save.svg), ISC |
| icons/settings.svg | crates/oc-kit/assets/icons/settings.svg |
| icons/square.svg | translation-inference addition from Lucide 1.72.0 (lucide-icons/lucide, icons/square.svg), ISC |
| icons/sun.svg | crates/oc-kit/assets/icons/sun.svg |
| icons/text-quote.svg | translation-inference addition from Lucide 1.72.0 (lucide-icons/lucide, icons/text-quote.svg), ISC |
| icons/trash-2.svg | crates/oc-kit/assets/icons/trash-2.svg |
| icons/triangle-alert.svg | crates/oc-kit/assets/icons/triangle-alert.svg |
| icons/undo-2.svg | crates/oc-kit/assets/icons/undo-2.svg |
| icons/upload.svg | translation-inference addition from Lucide 1.72.0 (lucide-icons/lucide, icons/upload.svg), ISC |
| icons/volume-2.svg | translation-inference addition from Lucide 1.72.0 (lucide-icons/lucide, icons/volume-2.svg), ISC |
| icons/x.svg | crates/oc-kit/assets/icons/x.svg |
| js/busy.js | crates/oc-kit/assets/js/busy.js |
| js/copy.js | crates/oc-kit/assets/js/copy.js |
| js/keys.js | crates/oc-kit/assets/js/keys.js |
| js/menu.js | crates/oc-kit/assets/js/menu.js |
| js/notices.js | crates/oc-kit/assets/js/notices.js |
| js/theme.js | crates/oc-kit/assets/js/theme.js |

The Lucide additions are taken from release tag 1.72.0, commit 03eae6069edfc373673bd05d723491f05ba936e3. Their shapes are unchanged; only the wrapping `<svg>` attributes are rewritten to the manifest's icon format (`class="oc-icon"`, `aria-hidden`, `focusable`, no width, height or stroke-width).

Not copied: htmx and its SSE extension (no page here swaps fragments), prefs.js (it posts to the kit's server endpoint; the app has its own `assets/js/prefs.js`), dialog.js, live.js, pane.js, passkey.js, rows.js, and the workbench, tables and charts CSS.
