// Shared by the page scripts: API calls, event streams, notices, selects, recording and playback.

export const store = {
  get(key) {
    try {
      return localStorage.getItem(`ti.${key}`);
    } catch {
      return null;
    }
  },
  set(key, value) {
    try {
      if (value === null || value === "") localStorage.removeItem(`ti.${key}`);
      else localStorage.setItem(`ti.${key}`, value);
    } catch {
      /* storage blocked: the choice lasts for this page only */
    }
  },
};

// The API's one-sentence error, or the fallback when the body is not the error JSON.
export async function errorMessage(res, fallback) {
  try {
    const body = await res.json();
    if (typeof body?.error?.message === "string") return body.error.message;
  } catch {
    /* not JSON */
  }
  return fallback;
}

export class ApiError extends Error {}

export async function postJson(path, body, { signal } = {}) {
  const res = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify(body),
    signal,
  });
  if (!res.ok) throw new ApiError(await errorMessage(res, "That did not work. Try again."));
  return res;
}

// Splits a server-sent event stream into {event, data} records. Feed it text as it arrives.
export class EventParser {
  constructor() {
    this.buffer = "";
  }

  push(text) {
    this.buffer += text.replace(/\r\n?/g, "\n");
    const out = [];
    let end;
    while ((end = this.buffer.indexOf("\n\n")) >= 0) {
      const block = this.buffer.slice(0, end);
      this.buffer = this.buffer.slice(end + 2);
      let event = "message";
      const data = [];
      for (const line of block.split("\n")) {
        if (line.startsWith(":")) continue;
        const colon = line.indexOf(":");
        const field = colon < 0 ? line : line.slice(0, colon);
        let value = colon < 0 ? "" : line.slice(colon + 1);
        if (value.startsWith(" ")) value = value.slice(1);
        if (field === "event") event = value;
        else if (field === "data") data.push(value);
      }
      if (data.length) out.push({ event, data: data.join("\n") });
    }
    return out;
  }
}

// Reads an event-stream response, calling onEvent(name, parsedData) for each event.
export async function readEvents(res, onEvent) {
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  const parser = new EventParser();
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    for (const e of parser.push(decoder.decode(value, { stream: true }))) {
      let data;
      try {
        data = JSON.parse(e.data);
      } catch {
        continue;
      }
      onEvent(e.event, data);
    }
  }
}

function statusIcon(kind) {
  const t = document.getElementById("ti-notice-icons");
  return t?.content.querySelector(`[data-kind="${kind}"] > svg`)?.cloneNode(true) ?? null;
}

// A floating notice: symbol, sentence, an optional action. It hides after 6 s unless hovered.
export function notify(kind, message, action) {
  const region = document.getElementById("oc-notices");
  if (!region) return null;
  const box = document.createElement("div");
  box.className = `oc-notice oc-notice--${kind}`;
  box.setAttribute("role", kind === "danger" ? "alert" : "status");
  box.dataset.ocAutohide = kind === "danger" ? "10000" : "6000";
  const icon = statusIcon(kind);
  if (icon) box.append(icon);
  const text = document.createElement("span");
  text.className = "oc-notice__text";
  text.textContent = message;
  box.append(text);
  if (action?.href) {
    const a = document.createElement("a");
    a.href = action.href;
    a.textContent = action.label;
    a.rel = "noopener";
    a.target = "_blank";
    box.append(a);
  } else if (action) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "oc-btn";
    b.textContent = action.label;
    b.addEventListener("click", () => {
      box.remove();
      action.run();
    });
    box.append(b);
  }
  region.append(box);
  return box;
}

// Sets a select and, when the kit has upgraded it to a listbox, the button that shows it.
export function setSelect(select, value) {
  if (![...select.options].some((o) => o.value === value)) return false;
  select.value = value;
  const wrap = select.nextElementSibling;
  if (wrap?.classList.contains("oc-menu-wrap")) {
    const button = wrap.querySelector(".oc-menu-button");
    if (button) button.textContent = select.options[select.selectedIndex]?.textContent ?? "";
    wrap.querySelectorAll('[role="option"]').forEach((li, i) => li.setAttribute("aria-selected", String(i === select.selectedIndex)));
  }
  return true;
}

// A target-language select with an "Other language" choice that reveals a text field.
export function targetPicker(select, other, key) {
  const sync = () => {
    other.hidden = select.value !== "other";
  };
  const saved = store.get(key);
  if (saved && !setSelect(select, saved)) {
    setSelect(select, "other");
    other.value = saved;
  }
  sync();
  select.addEventListener("change", () => {
    sync();
    if (select.value === "other") other.focus();
    else store.set(key, select.value);
  });
  other.addEventListener("change", () => store.set(key, other.value.trim()));
  return () => (select.value === "other" ? other.value.trim() : select.value);
}

// Shows aria-busy on a button only after 400 ms, so quick answers do not flicker.
export function busy(button) {
  const timer = setTimeout(() => button?.setAttribute("aria-busy", "true"), 400);
  return () => {
    clearTimeout(timer);
    button?.removeAttribute("aria-busy");
  };
}

export function download(name, text, type = "text/plain") {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export async function transcribe(file, language) {
  const form = new FormData();
  form.append("file", file, file.name || "recording");
  if (language && language !== "auto") form.append("language", language);
  const res = await fetch("/api/v1/transcribe", { method: "POST", body: form, headers: { Accept: "application/json" } });
  if (!res.ok) throw new ApiError(await errorMessage(res, "The recording could not be transcribed. Try again."));
  return (await res.json()).text;
}

// Voice-activity detection: stop once speech was heard and then silence lasted long enough.
export const VAD = { threshold: 0.015, minSpeechMs: 400, graceMs: 1500 };

export function vadStep(state, rms, now) {
  const s = { ...state };
  if (rms > VAD.threshold) {
    s.speechStart ??= now;
    s.silenceStart = null;
    return { state: s, stop: false };
  }
  s.silenceStart ??= now;
  const spoke = s.speechStart !== null && s.speechStart !== undefined && s.silenceStart - s.speechStart >= VAD.minSpeechMs;
  return { state: s, stop: spoke && now - s.silenceStart >= VAD.graceMs };
}

function micMessage(e) {
  if (e?.name === "NotAllowedError") return "Microphone access was denied. Allow it in the browser and try again.";
  if (e?.name === "NotFoundError") return "No microphone was found.";
  return "Recording could not start in this browser.";
}

// Records from the microphone until stop() or, with vad, a pause. Resolves with the audio file,
// or null when nothing was recorded.
export async function record({ vad = false, onStart } = {}) {
  if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
    throw new ApiError("This browser cannot record audio.");
  }
  let stream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: true });
  } catch (e) {
    throw new ApiError(micMessage(e));
  }
  const recorder = new MediaRecorder(stream);
  const chunks = [];
  recorder.addEventListener("dataavailable", (e) => e.data.size > 0 && chunks.push(e.data));
  const stop = () => recorder.state === "recording" && recorder.stop();
  const done = new Promise((resolve) => {
    recorder.addEventListener("stop", () => {
      stream.getTracks().forEach((t) => t.stop());
      const type = recorder.mimeType || "audio/webm";
      const ext = type.includes("ogg") ? "ogg" : type.includes("mp4") ? "m4a" : "webm";
      const blob = new Blob(chunks, { type });
      resolve(blob.size ? new File([blob], `recording.${ext}`, { type }) : null);
    });
  });
  recorder.start();
  onStart?.(stop);
  if (vad) {
    try {
      const ctx = new AudioContext();
      const analyser = ctx.createAnalyser();
      analyser.fftSize = 512;
      ctx.createMediaStreamSource(stream).connect(analyser);
      const buf = new Float32Array(analyser.fftSize);
      let state = { speechStart: null, silenceStart: null };
      const tick = () => {
        if (recorder.state !== "recording") return ctx.close();
        analyser.getFloatTimeDomainData(buf);
        const rms = Math.sqrt(buf.reduce((sum, v) => sum + v * v, 0) / buf.length);
        const step = vadStep(state, rms, performance.now());
        state = step.state;
        if (step.stop) {
          stop();
          return ctx.close();
        }
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    } catch {
      /* no AudioContext: the speaker stops by pressing the button again */
    }
  }
  return done;
}

// One voice at a time across the page.
let playing = null;

export function stopSpeech() {
  if (!playing) return;
  playing.audio.pause();
  URL.revokeObjectURL(playing.url);
  playing.onEnd?.();
  playing = null;
}

export async function speak(text, language, onEnd) {
  stopSpeech();
  const res = await postJson("/api/v1/speech", { text, language });
  const url = URL.createObjectURL(await res.blob());
  const audio = new Audio(url);
  playing = { audio, url, onEnd };
  audio.addEventListener("ended", () => playing?.audio === audio && stopSpeech());
  await audio.play();
}

export function isPlaying() {
  return playing !== null;
}
