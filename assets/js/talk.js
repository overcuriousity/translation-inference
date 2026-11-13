// The Talk page: two people take turns; each turn is transcribed, translated and read aloud.
import { ApiError, busy, download, notify, postJson, record, setSelect, speak, store, transcribe } from "./common.js";

export function exportText(turns) {
  return turns.map((t) => `${t.fromName}: ${t.source}\n→ ${t.translation}`).join("\n\n");
}

const root = typeof document !== "undefined" ? document.getElementById("ti-talk") : null;

if (root) {
  const $ = (id) => document.getElementById(id);
  const log = $("ti-talk-log");
  const turnTemplate = $("ti-turn");
  const voices = new Set((root.dataset.speech || "").split(" ").filter(Boolean));
  const autoRead = $("ti-auto-read");
  const vad = $("ti-vad");
  const langs = { a: $("ti-lang-a"), b: $("ti-lang-b") };
  const mics = { a: $("ti-mic-a"), b: $("ti-mic-b") };
  const exportButton = $("ti-export");
  const clearButton = $("ti-clear-talk");
  let turns = [];
  let stopRecording = null;

  for (const side of ["a", "b"]) {
    const saved = store.get(`talk.${side}`);
    if (saved) setSelect(langs[side], saved);
    langs[side].addEventListener("change", () => store.set(`talk.${side}`, langs[side].value));
  }
  if (autoRead) {
    autoRead.checked = store.get("talk.read") !== "0";
    autoRead.addEventListener("change", () => store.set("talk.read", autoRead.checked ? null : "0"));
  }
  vad.checked = store.get("talk.vad") !== "0";
  vad.addEventListener("change", () => store.set("talk.vad", vad.checked ? null : "0"));

  const name = (select) => select.options[select.selectedIndex]?.textContent ?? select.value;

  function render() {
    log.replaceChildren(
      ...turns.map((t) => {
        const li = turnTemplate.content.firstElementChild.cloneNode(true);
        li.classList.add(`ti-turn--${t.side}`);
        li.querySelector(".ti-turn__lang").textContent = `${t.fromName} → ${t.toName}`;
        li.querySelector(".ti-turn__source").textContent = t.source;
        li.querySelector(".ti-turn__translation").textContent = t.translation;
        const play = li.querySelector(".ti-turn__play");
        if (play && voices.has(t.to)) {
          play.hidden = false;
          play.addEventListener("click", () => readAloud(t));
        }
        return li;
      }),
    );
    exportButton.disabled = clearButton.disabled = turns.length === 0;
    log.lastElementChild?.scrollIntoView({ block: "nearest" });
  }

  async function readAloud(t) {
    try {
      await speak(t.translation, t.to);
    } catch (e) {
      notify("danger", e instanceof ApiError ? e.message : "Reading aloud did not work.");
    }
  }

  function label(side, recording) {
    const b = mics[side];
    b.setAttribute("aria-pressed", String(recording));
    b.querySelector("span").textContent = recording ? "Stop" : "Speak";
  }

  async function turn(side) {
    if (stopRecording) return stopRecording();
    const other = side === "a" ? "b" : "a";
    const from = langs[side].value;
    const to = langs[other].value;
    mics[other].disabled = true;
    let done = () => {};
    try {
      const file = await record({
        vad: vad.checked,
        onStart: (stop) => {
          stopRecording = stop;
          label(side, true);
        },
      });
      stopRecording = null;
      label(side, false);
      if (!file) return notify("info", "Nothing was recorded.");
      done = busy(mics[side]);
      const source = (await transcribe(file, from)).trim();
      if (!source) return notify("info", "No speech was heard.");
      const res = await postJson("/api/v1/translate", { text: source, source: from, target: to });
      const { text } = await res.json();
      const t = { side, from, to, fromName: name(langs[side]), toName: name(langs[other]), source, translation: text.trim() };
      turns.push(t);
      render();
      if (autoRead?.checked && voices.has(to)) readAloud(t);
    } catch (e) {
      notify("danger", e instanceof ApiError ? e.message : "That turn did not work. Try again.");
    } finally {
      stopRecording = null;
      label(side, false);
      mics[other].disabled = false;
      done();
    }
  }

  mics.a.addEventListener("click", () => turn("a"));
  mics.b.addEventListener("click", () => turn("b"));

  clearButton.addEventListener("click", () => {
    const before = turns;
    turns = [];
    render();
    notify("info", "Conversation cleared", {
      label: "Undo",
      run: () => {
        turns = before;
        render();
      },
    });
  });
  exportButton.addEventListener("click", () => download("conversation.txt", exportText(turns)));
  render();
}
