# Plan: Upcoming Features

Five features in scope. Ordered by dependency and implementation cost.

---

## 1. Side-by-Side Paragraph View

**What:** Replace the single output blob with a two-column aligned view —
source paragraph on the left, translated paragraph on the right. Readers can
follow along sentence by sentence and spot mistranslations immediately.

**API change — new endpoint `POST /api/translate/paragraphs`**

Returns structured data instead of a flat string:

```json
{
  "paragraphs": [
    { "source": "Erster Absatz.", "translation": "First paragraph." },
    { "source": "Zweiter Absatz.", "translation": "Second paragraph." }
  ],
  "chunks_total": 1,
  "chunks_completed": 1
}
```

Implementation: split input on `\n\n` to get source paragraphs, run through
the existing `translate_paragraphs` batching logic, zip source + translated
slices into the response array. Empty paragraphs (spacing) are preserved as
`{ "source": "", "translation": "" }` pairs.

Streaming variant (`POST /api/translate/paragraphs/stream`) can emit
completed paragraphs as SSE events so the UI fills in progressively.

**UI change**

A toggle button in the output panel header switches between:
- **Flat mode** (current): single scrollable text div
- **Paragraph mode**: two-column `<table>` or CSS-grid layout, one row per
  paragraph pair, alternating row shading for readability

The toggle state is persisted in localStorage. Copy button in paragraph mode
copies the right column (translation only) as plain text joined by `\n\n`.

**Files:** `src/api/chat.rs`, `src/document/mod.rs` → `src/translation/mod.rs`
(or a new `src/routes/translate_paragraphs.rs`), `static/app.js`,
`static/index.html`, `static/style.css`

---

## 2. Markdown Rendering on Output

**What:** A toggle button on the output panel renders the translation as
formatted HTML using a lightweight client-side markdown parser. Raw text mode
remains the default; the button switches to rendered view.

**No backend change required.**

**Implementation**

- Include [marked.js](https://marked.js.org/) (single ~50 KB minified file)
  as a static asset.
- Add a "Render markdown" icon button next to the copy button in the output
  footer.
- When active: `outputDiv.innerHTML = marked.parse(translatedText)` with
  `sanitize: true` (or use DOMPurify).
- When inactive: restore `outputDiv.textContent = translatedText`.
- Copy button always copies the raw markdown string, not the rendered HTML.
- Toggle state persisted in localStorage.
- Auto-enable heuristic (optional): if the output contains `#`, `**`, ` ``` `
  or `- ` patterns, suggest enabling rendering via a subtle banner.

**Files:** `static/app.js`, `static/index.html`, `static/style.css`,
`static/marked.min.js` (new)

---

## 3. Domain / Context Hint

**What:** An optional free-text field ("medical", "legal contract",
"casual chat between friends") that is injected into the system prompt
to steer register, terminology, and tone. Intended for power users who
know their content domain.

**API change**

Add optional `context` field to `TranslateRequest` and the streaming
equivalent:

```rust
pub struct TranslateRequest {
    // existing fields …
    pub context: Option<String>,
}
```

In `build_system_prompt()` in `src/api/chat.rs`, append when present:

```
The text belongs to the following domain or context: {context}.
Adapt terminology and register accordingly.
```

The field is also forwarded through the paragraph endpoint added in feature 1.

**UI change**

A small, collapsible "Context hint" input row beneath the source panel
footer (hidden by default, revealed by a "⚙ Context" link or similar).
Value is persisted in localStorage across sessions.

**Files:** `src/models.rs`, `src/api/chat.rs`, `src/routes/translate.rs`,
`static/app.js`, `static/index.html`, `static/style.css`

---

## 4. SRT / Subtitle File Translation

**What:** Upload an `.srt` (or `.vtt`) subtitle file; receive a translated
subtitle file with all timestamps preserved. Reintroduces a minimal file
upload UI — a drop zone in the source panel that activates when the user
switches to a new **"File" tab** (distinct from the removed Document tab).

**Format**

SRT structure:
```
1
00:00:01,000 --> 00:00:04,000
Hello, how are you?

2
00:00:05,500 --> 00:00:08,200
I am fine, thank you.
```

Each cue's text block is extracted, translated as a batch (using the
separator logic already in `translate_paragraphs`), and written back.
Timestamps and sequence numbers are never touched.

VTT is similar; both share the same parsing logic with a format flag.

**Backend**

New endpoint `POST /api/translate-subtitle`:

```
multipart/form-data:
  file:        .srt or .vtt binary
  source_lang: string (default "auto")
  target_lang: string
  model:       string (optional)
  endpoint:    string (optional)
  api_key:     string (optional)
```

Response:
```json
{
  "filename": "subtitles_translated.srt",
  "data": "<base64-encoded translated SRT>",
  "mime": "text/plain"
}
```

Parser lives in `src/subtitle/mod.rs` (new module):

```rust
pub struct SubtitleCue {
    pub index: usize,         // sequence number (SRT) or None (VTT)
    pub timecode: String,     // raw timecode line, copied verbatim
    pub lines: Vec<String>,   // text lines (may be multi-line)
}

pub fn parse_srt(input: &str) -> Vec<SubtitleCue>
pub fn parse_vtt(input: &str) -> Vec<SubtitleCue>
pub fn render_srt(cues: &[SubtitleCue]) -> String
pub fn render_vtt(cues: &[SubtitleCue]) -> String
```

Translation: join each cue's `lines` into a single string, batch-translate
all cues using `translate_paragraphs`, write translated text back into
`cue.lines` (split on `\n` to preserve line wrapping if count matches, else
single block).

**UI — "File" tab**

A third tab in the source panel header: **Text | File | Conversation**
(Conversation described in feature 5).

File tab shows:
- A drop zone accepting `.srt` and `.vtt`
- After upload: filename badge + "Translate" button
- Language selector and model selector remain active
- Output panel shows a download button for the translated file (same
  base64-download pattern used previously in the upload route)

This tab's drop zone is the foundation for any future file-based features —
deliberately kept minimal so adding new file types later requires only a
format handler, not a UI redesign.

**Files:** `src/subtitle/mod.rs` (new), `src/routes/subtitle.rs` (new),
`src/routes/mod.rs`, `src/main.rs`, `src/models.rs`, `static/app.js`,
`static/index.html`, `static/style.css`, `static/openapi.yaml`

---

## 5. Conversation Mode

**What:** A dedicated tab optimised for real-time, spoken dialogue between
two parties who speak different languages. Each party speaks into the
microphone; the app transcribes and translates immediately, showing an
alternating chat-bubble transcript. Designed for face-to-face interpreter
scenarios.

**Tab layout**

```
┌─────────────────────────────────────────┐
│  Text   File   Conversation             │
├─────────────────────────────────────────┤
│                                         │
│  [Person A — German]          ●         │  ← tap to record A
│  ┌─────────────────────────────────┐   │
│  │ Ich brauche Hilfe.              │   │
│  │ → I need help.                  │   │
│  └─────────────────────────────────┘   │
│                                         │
│  [Person B — English]         ●         │  ← tap to record B
│  ┌─────────────────────────────────┐   │
│  │ What kind of help do you need?  │   │
│  │ → Was für Hilfe brauchen Sie?   │   │
│  └─────────────────────────────────┘   │
│                                         │
│            [Clear conversation]         │
└─────────────────────────────────────────┘
```

**State**

```js
let convHistory = [];       // [{speaker: 'A'|'B', source, translation}]
let convLangA   = 'de';
let convLangB   = 'en';
let convRecording = null;   // 'A' | 'B' | null
```

**Flow for one turn**

1. User taps Speaker A's mic button → `mediaRecorder` starts.
2. On stop → POST to `/api/transcribe` (existing Whisper endpoint).
3. Transcribed text → POST to `/api/translate` with `source_lang = convLangA`,
   `target_lang = convLangB` (name resolved from code).
4. Append bubble to `convHistory`, re-render the transcript.
5. Auto-scroll to bottom.

Both speakers share the same Whisper model and translation model selectors
from the header. Language pickers for A and B are inline in the conversation
tab (dropdowns or the existing lang-picker component reused twice).

**No backend changes required** — the conversation tab is entirely
orchestrated from existing `/api/transcribe` and `/api/translate` calls.

**Considerations**

- On mobile/tablet the two mic buttons should be large and thumb-friendly.
- A "Push to talk" mode (hold button) vs. "Tap to start / tap to stop" mode —
  both should be configurable (localStorage toggle).
- The chat transcript can be exported as plain text (speaker label + lines).
- Auto-language detection (`source_lang: "auto"`) can be offered as an
  option instead of fixed per-speaker languages, useful when both speakers
  may switch languages unpredictably.

**Files:** `static/app.js`, `static/index.html`, `static/style.css`
(backend: none)

---

## Implementation Order

```
1. Markdown rendering          — pure frontend, no API, quick win
2. Side-by-side paragraphs     — new API endpoint, then UI toggle
3. Domain/context hint         — small API + prompt change, then UI
4. SRT subtitle translation    — new backend module + File tab UI
5. Conversation mode           — pure frontend, builds on File tab's tab structure
```

Steps 1–3 are independent of each other and can be parallelised.
Step 4 introduces the File tab that step 5's Conversation tab sits alongside.

---

## Dependency Summary

| Feature | New backend | New frontend | New static assets |
|---|---|---|---|
| Paragraph view | New route | Toggle button, grid layout | — |
| Markdown rendering | None | Toggle button, render logic | `marked.min.js` |
| Context hint | `context` field in prompt | Collapsible input | — |
| SRT translation | New module + route | File tab, download button | — |
| Conversation mode | None | New tab, bubble UI | — |
