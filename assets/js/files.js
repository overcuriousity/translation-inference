// The Files page: subtitle files translated one after another, each downloadable when done.
import { errorMessage, download, notify, readEvents, store, targetPicker } from "./common.js";

const root = typeof document !== "undefined" ? document.getElementById("ti-files") : null;

if (root) {
  const $ = (id) => document.getElementById(id);
  const drop = $("ti-drop");
  const input = $("ti-file");
  const list = $("ti-file-list");
  const rowTemplate = $("ti-file-row");
  const context = $("ti-context");
  const target = targetPicker($("ti-target-lang"), $("ti-target-other"), "target");
  context.value = store.get("files.context") ?? "";
  context.addEventListener("change", () => store.set("files.context", context.value.trim()));

  const queue = [];
  let working = false;

  function status(row, kind, text) {
    const meta = row.querySelector(".oc-list__meta");
    meta.replaceChildren();
    const icon = kind && $("ti-status-icons").content.querySelector(`[data-kind="${kind}"]`)?.cloneNode(true);
    if (icon) meta.append(icon);
    meta.append(document.createTextNode(text));
  }

  function add(files) {
    const lang = target();
    if (!lang) {
      notify("warning", "Enter the language to translate to.");
      return;
    }
    for (const file of files) {
      const row = rowTemplate.content.firstElementChild.cloneNode(true);
      row.querySelector(".oc-list__title").textContent = file.name;
      status(row, null, "Waiting");
      list.append(row);
      queue.push({ file, row, lang, context: context.value.trim() });
    }
    work();
  }

  async function translateOne({ file, row, lang, context }) {
    const form = new FormData();
    form.append("target", lang);
    if (context) form.append("context", context);
    form.append("file", file, file.name);
    status(row, null, "Starting");
    const res = await fetch("/api/v1/subtitles", { method: "POST", body: form });
    if (!res.ok) {
      status(row, "danger", await errorMessage(res, "This file could not be translated."));
      return;
    }
    let finished = false;
    await readEvents(res, (event, data) => {
      if (event === "progress") status(row, null, `${data.done} / ${data.total}`);
      else if (event === "error") status(row, "danger", data.message);
      else if (event === "done") {
        finished = true;
        status(row, "success", "Done");
        const button = row.querySelector("button");
        button.hidden = false;
        button.setAttribute("aria-label", `Download ${data.filename}`);
        button.dataset.ocTip = `Download ${data.filename}`;
        button.addEventListener("click", () => download(data.filename, data.content));
      }
    });
    if (!finished && !row.querySelector('[data-kind="danger"]')) status(row, "danger", "The translation broke off. Try again.");
  }

  async function work() {
    if (working) return;
    working = true;
    while (queue.length) {
      const job = queue.shift();
      try {
        await translateOne(job);
      } catch {
        status(job.row, "danger", "The connection broke off. Try again.");
      }
    }
    working = false;
  }

  input.addEventListener("change", () => {
    add([...input.files]);
    input.value = "";
  });
  for (const type of ["dragenter", "dragover"]) {
    drop.addEventListener(type, (e) => {
      e.preventDefault();
      drop.dataset.over = "";
    });
  }
  for (const type of ["dragleave", "drop"]) drop.addEventListener(type, () => delete drop.dataset.over);
  drop.addEventListener("drop", (e) => {
    e.preventDefault();
    const files = [...(e.dataTransfer?.files ?? [])].filter((f) => /\.(srt|vtt)$/i.test(f.name));
    if (files.length) add(files);
    else notify("warning", "Drop .srt or .vtt files.");
  });
}
