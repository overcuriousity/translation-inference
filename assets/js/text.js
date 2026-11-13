// The Text page: translate as you watch, detect the source language, dictate, read aloud.
import {
  ApiError, busy, isPlaying, notify, postJson, readEvents, record, setSelect, speak, stopSpeech, store, targetPicker, transcribe,
} from "./common.js";

// Detection runs again only when the text changed this much.
export function significantChange(text, last) {
  if (!last) return true;
  const threshold = Math.max(50, Math.floor(last.length * 0.2));
  return Math.abs(text.length - last.length) > threshold || text.slice(0, 100) !== last.slice(0, 100);
}

export function countLabel(n, limit) {
  return limit ? `${n.toLocaleString("en")} / ${limit.toLocaleString("en")}` : n ? n.toLocaleString("en") : "";
}

const root = typeof document !== "undefined" ? document.getElementById("ti-translate") : null;

if (root) {
  const $ = (id) => document.getElementById(id);
  const source = $("ti-source");
  const sourceLang = $("ti-source-lang");
  const detected = $("ti-detected");
  const targetLang = $("ti-target-lang");
  const output = $("ti-output");
  const paragraphs = $("ti-paragraphs");
  const go = $("ti-go");
  const count = $("ti-count");
  const progress = $("ti-progress");
  const incomplete = $("ti-incomplete");
  const context = $("ti-context");
  const copy = $("ti-copy");
  const mdButton = $("ti-markdown");
  const sideButton = $("ti-side");
  const outputTools = [mdButton, sideButton, copy, $("ti-save-output")].filter(Boolean);
  const limit = Number(root.dataset.limit) || 0;
  const voices = new Set((root.dataset.speech || "").split(" ").filter(Boolean));

  const target = targetPicker(targetLang, $("ti-target-other"), "target");
  context.value = store.get("context") ?? "";
  context.addEventListener("change", () => store.set("context", context.value.trim()));

  let text = ""; // the translation as plain text
  let markdown = store.get("markdown") === "1";
  let side = store.get("side") === "1";
  let detectedCode = null;
  let lastDetected = "";
  let detectTimer;
  let detectId = 0;
  let running = null;

  const press = (b, on) => b?.setAttribute("aria-pressed", String(on));
  press(mdButton, markdown);
  press(sideButton, side);

  const sourceCode = () => (sourceLang.value === "auto" ? detectedCode : sourceLang.value);

  function syncVoices() {
    const rs = $("ti-read-source");
    const ro = $("ti-read-output");
    if (rs) rs.hidden = !source.value.trim() || !voices.has(sourceCode() ?? "");
    if (ro) ro.hidden = !text || !voices.has(target());
  }

  function syncCount() {
    const n = source.value.length;
    count.textContent = countLabel(n, limit);
    const over = limit > 0 && n > limit;
    count.toggleAttribute("data-over", over);
    go.disabled = over || !source.value.trim();
  }

  function showText() {
    output.classList.remove("ti-output--markdown");
    output.textContent = text;
  }

  function setOutput(t) {
    text = t;
    copy.dataset.ocCopy = t;
    showText();
    for (const b of outputTools) b.disabled = !t;
    syncVoices();
  }

  async function renderMarkdown() {
    if (!markdown || !text) return showText();
    try {
      const res = await postJson("/markdown", { text });
      const { html } = await res.json();
      // The server escapes raw HTML and drops script links; the page's CSP blocks the rest.
      const doc = new DOMParser().parseFromString(html, "text/html");
      output.classList.add("ti-output--markdown");
      output.replaceChildren(...doc.body.childNodes);
    } catch {
      showText();
    }
  }

  async function renderSide() {
    paragraphs.hidden = !side || !text;
    output.hidden = side && !!text;
    if (!side || !text) return;
    paragraphs.replaceChildren();
    const done = busy(sideButton);
    try {
      const res = await postJson("/api/v1/translate/paragraphs", request());
      for (const p of await res.json()) {
        if (!p.source.trim()) continue;
        const row = document.createElement("div");
        const a = document.createElement("p");
        const b = document.createElement("p");
        a.textContent = p.source;
        b.textContent = p.translation;
        row.append(a, b);
        paragraphs.append(row);
      }
    } catch (e) {
      notify("danger", e instanceof ApiError ? e.message : "Side by side did not work. Try again.");
      side = false;
      press(sideButton, false);
      paragraphs.hidden = true;
      output.hidden = false;
    } finally {
      done();
    }
  }

  function request() {
    return { text: source.value, target: target(), source: sourceLang.value, context: context.value.trim() || undefined };
  }

  async function translate() {
    if (!source.value.trim() || go.disabled) return;
    if (!target()) {
      notify("warning", "Enter the language to translate to.");
      $("ti-target-other").focus();
      return;
    }
    running?.abort();
    const abort = new AbortController();
    running = abort;
    stopSpeech();
    setOutput("");
    output.hidden = false;
    paragraphs.hidden = true;
    incomplete.hidden = true;
    progress.hidden = true;
    output.setAttribute("aria-busy", "true");
    const done = busy(go);
    let failed = false;
    try {
      const res = await postJson("/api/v1/translate/stream", request(), { signal: abort.signal });
      await readEvents(res, (event, data) => {
        if (abort.signal.aborted) return;
        if (event === "chunk" && data.total > 1) {
          progress.hidden = false;
          progress.textContent = `${data.index} / ${data.total}`;
        } else if (event === "text") {
          text += data.text;
          output.textContent = text;
        } else if (event === "truncated") {
          incomplete.hidden = false;
        } else if (event === "error") {
          failed = true;
          notify("danger", data.message);
        }
      });
    } catch (e) {
      if (e.name === "AbortError") return;
      failed = true;
      notify("danger", e instanceof ApiError ? e.message : "The translation broke off. Try again.");
    } finally {
      if (running === abort) {
        running = null;
        done();
        output.removeAttribute("aria-busy");
        progress.hidden = true;
      }
    }
    if (abort.signal.aborted) return;
    setOutput(text);
    if (!failed && text) {
      await renderMarkdown();
      await renderSide();
    }
  }

  async function detect(snippet) {
    const id = ++detectId;
    try {
      const res = await postJson("/api/v1/detect", { text: snippet.slice(0, 500) });
      const { language } = await res.json();
      if (id !== detectId) return;
      const option = [...sourceLang.options].find((o) => o.value.toLowerCase() === language.toLowerCase());
      detectedCode = option?.value ?? null;
      lastDetected = snippet;
      detected.textContent = option ? option.textContent : "";
      detected.hidden = !option || sourceLang.value !== "auto";
    } catch {
      if (id === detectId) detected.hidden = true;
    }
    syncVoices();
  }

  function scheduleDetect() {
    const t = source.value;
    if (t.trim().length < 10) {
      detectId++;
      detectedCode = null;
      lastDetected = "";
      detected.hidden = true;
      return;
    }
    if (sourceLang.value !== "auto" || !significantChange(t, lastDetected)) return;
    clearTimeout(detectTimer);
    detectTimer = setTimeout(() => detect(t), 500);
  }

  source.addEventListener("input", () => {
    syncCount();
    scheduleDetect();
    syncVoices();
  });
  source.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      translate();
    }
  });
  go.addEventListener("click", translate);
  sourceLang.addEventListener("change", () => {
    detected.hidden = sourceLang.value !== "auto" || !detectedCode;
    scheduleDetect();
    syncVoices();
  });
  targetLang.addEventListener("change", () => {
    syncVoices();
    if (text && targetLang.value !== "other") translate();
  });
  $("ti-target-other").addEventListener("change", () => text && translate());

  $("ti-swap").addEventListener("click", () => {
    const from = sourceCode();
    const to = target();
    if (!from || !setSelect(sourceLang, to)) {
      notify("info", from ? "Only languages from the list can be swapped." : "Choose the source language first.");
      return;
    }
    setSelect(targetLang, from);
    $("ti-target-other").hidden = true;
    store.set("target", from);
    if (text) {
      source.value = text;
      setOutput("");
    }
    detected.hidden = true;
    syncCount();
    if (source.value.trim()) translate();
  });

  $("ti-clear").addEventListener("click", () => {
    const before = { source: source.value, text };
    if (!before.source && !before.text) return;
    running?.abort();
    source.value = "";
    setOutput("");
    syncCount();
    scheduleDetect();
    source.focus();
    notify("info", "Cleared", {
      label: "Undo",
      run: () => {
        source.value = before.source;
        setOutput(before.text);
        syncCount();
        scheduleDetect();
        renderMarkdown();
      },
    });
  });

  mdButton.addEventListener("click", () => {
    markdown = !markdown;
    press(mdButton, markdown);
    store.set("markdown", markdown ? "1" : null);
    renderMarkdown();
  });
  sideButton.addEventListener("click", () => {
    side = !side;
    press(sideButton, side);
    store.set("side", side ? "1" : null);
    renderSide();
  });

  function readAloud(button, getText, getLang) {
    button?.addEventListener("click", async () => {
      if (button.getAttribute("aria-pressed") === "true" && isPlaying()) return stopSpeech();
      const done = busy(button);
      try {
        await speak(getText(), getLang(), () => press(button, false));
        press(button, true);
      } catch (e) {
        press(button, false);
        notify("danger", e instanceof ApiError ? e.message : "Reading aloud did not work.");
      } finally {
        done();
      }
    });
  }
  readAloud($("ti-read-source"), () => source.value, () => sourceCode());
  readAloud($("ti-read-output"), () => text, () => target());

  function saveTo(button, getText) {
    button?.addEventListener("click", async () => {
      const done = busy(button);
      try {
        const res = await postJson("/api/v1/bitvault", { text: getText() });
        const { url } = await res.json();
        notify("success", "Saved to BitVault", { href: url, label: "Open" });
      } catch (e) {
        notify("danger", e instanceof ApiError ? e.message : "Saving did not work. Try again.");
      } finally {
        done();
      }
    });
  }
  saveTo($("ti-save-source"), () => source.value);
  saveTo($("ti-save-output"), () => text);

  function insert(transcript) {
    if (!transcript.trim()) return notify("info", "No speech was heard.");
    const sep = source.value && !/\s$/.test(source.value) ? "\n" : "";
    source.value += sep + transcript;
    syncCount();
    scheduleDetect();
    source.focus();
  }

  const mic = $("ti-mic");
  let stopRecording = null;
  mic?.addEventListener("click", async () => {
    if (stopRecording) return stopRecording();
    let done = () => {};
    try {
      const file = await record({
        onStart: (stop) => {
          stopRecording = stop;
          press(mic, true);
          mic.setAttribute("aria-label", "Stop dictating");
          mic.dataset.ocTip = "Stop dictating";
        },
      });
      stopRecording = null;
      press(mic, false);
      mic.setAttribute("aria-label", "Dictate");
      mic.dataset.ocTip = "Dictate";
      if (!file) return notify("info", "Nothing was recorded.");
      done = busy(mic);
      insert(await transcribe(file, sourceLang.value));
    } catch (e) {
      notify("danger", e instanceof ApiError ? e.message : "Dictation did not work. Try again.");
    } finally {
      stopRecording = null;
      press(mic, false);
      done();
    }
  });

  const media = $("ti-media");
  media?.addEventListener("change", async () => {
    const file = media.files?.[0];
    media.value = "";
    if (!file) return;
    const label = document.querySelector('label[for="ti-media"]');
    const done = busy(label);
    const note = notify("info", `Transcribing ${file.name}`);
    try {
      insert(await transcribe(file, sourceLang.value));
    } catch (e) {
      notify("danger", e instanceof ApiError ? e.message : "The recording could not be transcribed.");
    } finally {
      note?.remove();
      done();
    }
  });

  syncCount();
  scheduleDetect();
  syncVoices();
}
