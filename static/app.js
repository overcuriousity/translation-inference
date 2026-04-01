'use strict';

// ── State ────────────────────────────────────────────────────────────────
let availableLanguages = [];
let availableModels    = [];
let userEndpoint       = '';
let userApiKey         = '';
let translationTimeout = null;
let lastTranslatedText = '';

// ── DOM refs ─────────────────────────────────────────────────────────────
const sourceLangSel    = document.getElementById('source-lang');
const targetLangSel    = document.getElementById('target-lang');
const detectedBadge    = document.getElementById('detected-lang');
const modelSel         = document.getElementById('model-select');
const whisperModelSel  = document.getElementById('whisper-model-select');
const sourceText       = document.getElementById('source-text');
const outputDiv        = document.getElementById('output-text');
const charCount        = document.getElementById('char-count');
const translateBtn     = document.getElementById('translate-btn');
const clearBtn         = document.getElementById('clear-btn');
const copyBtn          = document.getElementById('copy-btn');
const swapBtn          = document.getElementById('swap-btn');
const fileInput        = document.getElementById('file-input');
const resultsDocList    = document.getElementById('results-doc-list');
const transcribeStatus = document.getElementById('transcribe-status');
const chunkProgress    = document.getElementById('chunk-progress');
const notification     = document.getElementById('notification');
const configPanel      = document.getElementById('config-panel');
const configEndpoint   = document.getElementById('config-endpoint');
const configApikey     = document.getElementById('config-apikey');
const configConnectBtn = document.getElementById('config-connect-btn');
const configMsg        = document.getElementById('config-msg');
const settingsBtn      = document.getElementById('settings-btn');
const dropOverlay      = document.getElementById('drop-overlay');
const sourcePanel      = document.querySelector('.panel-source');
const saveSrcBtn       = document.getElementById('save-src-btn');
const saveOutBtn       = document.getElementById('save-out-btn');

// ── Boot ─────────────────────────────────────────────────────────────────
async function init() {
  const status = await fetch('/api/status').then(r => r.json())
    .catch(() => ({ server_configured: false, session_active: false, bitvault_configured: false }));

  if (status.bitvault_configured) {
    saveSrcBtn.classList.remove('hidden');
    saveOutBtn.classList.remove('hidden');
  }

  if (!status.server_configured && !status.session_active) {
    showConfigPanel('Please configure your API credentials.');
  } else {
    await Promise.all([loadLanguages(), loadModels()]);
  }

  // Pre-fill source text from a Bitvault paste URL passed as ?from=
  const fromUrl = new URLSearchParams(window.location.search).get('from');
  if (fromUrl && status.bitvault_configured) {
    try {
      const res = await fetch(`/api/proxy-text?url=${encodeURIComponent(fromUrl)}`);
      if (res.ok) {
        sourceText.value = await res.text();
        updateCharCount();
        if (status.server_configured || status.session_active) {
          clearTimeout(translationTimeout);
          translate(true);
        } else {
          showConfigPanel('Please configure your API credentials to enable auto-translation.');
        }
      }
    } catch (_) { /* silently ignore */ }
  }
}

// ── Config panel ──────────────────────────────────────────────────────────
function showConfigPanel(msg) {
  configPanel.classList.remove('hidden');
  if (msg) {
    configMsg.textContent = msg;
    configMsg.className = 'config-msg';
  }
}

settingsBtn.addEventListener('click', () => {
  configPanel.classList.toggle('hidden');
  configMsg.textContent = '';
});

configConnectBtn.addEventListener('click', async () => {
  const ep  = configEndpoint.value.trim();
  const key = configApikey.value.trim();

  if (!ep || !key) {
    configMsg.textContent = 'Required fields missing.';
    configMsg.className = 'config-msg error';
    return;
  }

  configConnectBtn.disabled = true;
  configMsg.textContent = 'Testing…';

  try {
    const res  = await fetch('/api/config/test', {
      method:  'POST',
      headers: { 'Content-Type': 'application/json' },
      body:    JSON.stringify({ endpoint: ep, api_key: key }),
    });
    const data = await res.json();

    if (res.ok) {
      userEndpoint = ep;
      userApiKey   = key;
      configMsg.textContent = '✓ Connected';
      configMsg.className = 'config-msg success';
      await Promise.all([loadLanguages(), loadModels()]);
      setTimeout(() => configPanel.classList.add('hidden'), 800);
    } else {
      configMsg.textContent = data.error || 'Connection failed';
      configMsg.className = 'config-msg error';
    }
  } catch (e) {
    configMsg.textContent = 'Network error';
    configMsg.className = 'config-msg error';
  } finally {
    configConnectBtn.disabled = false;
  }
});

// ── Credentials helper ────────────────────────────────────────────────────
function apiCredentials() {
  return (userEndpoint && userApiKey) ? { endpoint: userEndpoint, api_key: userApiKey } : {};
}

function appendCredentialsToForm(form) {
  if (userEndpoint) form.append('endpoint', userEndpoint);
  if (userApiKey) form.append('api_key', userApiKey);
}

// ── Language / model loading ──────────────────────────────────────────────
async function loadLanguages() {
  try {
    const res  = await fetch('/api/languages');
    const data = await res.json();
    availableLanguages = data.languages;

    sourceLangSel.innerHTML = '';
    for (const lang of availableLanguages) {
      const opt = document.createElement('option');
      opt.value = lang.code; opt.textContent = lang.name;
      sourceLangSel.appendChild(opt);
    }

    targetLangSel.innerHTML = '';
    for (const lang of availableLanguages.filter(l => l.code !== 'auto')) {
      const opt = document.createElement('option');
      opt.value = lang.code; opt.textContent = lang.name;
      targetLangSel.appendChild(opt);
    }

    sourceLangSel.value = 'auto';
    targetLangSel.value = 'en';
  } catch (e) {
    showNotification('Language sync failed', 'error');
  }
}

async function loadModels() {
  try {
    const res  = await fetch('/api/models');
    const data = await res.json();

    // Translation models
    modelSel.innerHTML = '';
    if (data.translation_models) {
      for (const m of data.translation_models) {
        const opt = document.createElement('option');
        opt.value = m.id; opt.textContent = m.name;
        modelSel.appendChild(opt);
      }
    }

    // Transcription models
    whisperModelSel.innerHTML = '';
    if (data.transcription_models) {
      for (const m of data.transcription_models) {
        const opt = document.createElement('option');
        opt.value = m.id; opt.textContent = m.name;
        whisperModelSel.appendChild(opt);
      }
    }
  } catch (e) {
    console.error('loadModels error:', e);
    showNotification('Model sync failed', 'error');
  }
}

// ── Translation ───────────────────────────────────────────────────────────
async function translate(isAuto = false) {
  const text = sourceText.value.trim();
  if (!text) { setOutput(''); detectedBadge.classList.add('hidden'); return; }
  if (isAuto && text === lastTranslatedText) return;

  setOutputLoading(true);
  lastTranslatedText = text;

  try {
    const body = {
      text,
      source_lang: sourceLangName(sourceLangSel.value),
      target_lang: targetLangName(targetLangSel.value),
      model: modelSel.value || undefined,
      ...apiCredentials(),
    };

    const res  = await fetch('/api/translate/stream', {
      method:  'POST',
      headers: { 'Content-Type': 'application/json' },
      body:    JSON.stringify(body),
    });

    if (!res.ok) {
      if (!isAuto) showNotification('Translation error', 'error');
      setOutputLoading(false);
      return;
    }

    setOutput('');
    chunkProgress.textContent = 'Translating\u2026';
    chunkProgress.classList.remove('hidden');

    const reader = res.body.getReader();
    const decoder = new TextDecoder('utf-8');
    let textSoFar = '';
    let done = false;

    while (!done) {
      const { value, done: readerDone } = await reader.read();
      done = readerDone;
      if (value) {
        if (textSoFar === '') setOutputLoading(false);
        textSoFar += decoder.decode(value, { stream: true });
        setOutput(textSoFar);
      }
    }

    // The server sends \x00ERR:<message> as a sentinel when a mid-stream error
    // occurs (HTTP status is already 200 at that point and cannot be changed).
    const ERR_SENTINEL = '\x00ERR:';
    const errIdx = textSoFar.indexOf(ERR_SENTINEL);
    if (errIdx !== -1) {
      const partial = textSoFar.slice(0, errIdx);
      const errMsg  = textSoFar.slice(errIdx + ERR_SENTINEL.length).trim();
      setOutput(partial);
      if (!isAuto) showNotification(errMsg || 'Translation error', 'error');
    }

    chunkProgress.classList.add('hidden');
    detectedBadge.classList.add('hidden');

  } catch (e) {
    if (!isAuto) showNotification('Network error', 'error');
  } finally {
    setOutputLoading(false);
    chunkProgress.classList.add('hidden');
  }
}

function langNameByCode(code) {
  const found = availableLanguages.find(l => l.code === code);
  return found ? found.name : code;
}
function sourceLangName(code) { return code === 'auto' ? 'auto' : langNameByCode(code); }
function targetLangName(code) { return langNameByCode(code); }

// ── Debounced input ───────────────────────────────────────────────────────
sourceText.addEventListener('input', () => {
  updateCharCount();
  clearTimeout(translationTimeout);
  translationTimeout = setTimeout(() => translate(true), 600);
});

sourceText.addEventListener('keydown', e => {
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    clearTimeout(translationTimeout);
    translate();
  }
});

sourceLangSel.addEventListener('change', () => { lastTranslatedText = ''; translate(true); });
targetLangSel.addEventListener('change', () => { lastTranslatedText = ''; translate(true); });
modelSel.addEventListener('change', () => { lastTranslatedText = ''; translate(true); });

// ── Transcription & Upload ────────────────────────────────────────────────
async function handleFiles(files) {
  if (!files || files.length === 0) return;
  const fileArray = Array.from(files);
  sourceText.disabled = true;

  showPendingQueue(fileArray);

  let hasText = false;

  for (let i = 0; i < fileArray.length; i++) {
    const file = fileArray[i];
    try {
      const form = new FormData();
      form.append('file', file, file.name);
      form.append('source_lang', sourceLangName(sourceLangSel.value));
      form.append('target_lang', targetLangName(targetLangSel.value));
      form.append('model', modelSel.value || '');
      form.append('whisper_model', whisperModelSel.value || '');
      appendCredentialsToForm(form);

      const res  = await fetch('/api/upload', { method: 'POST', body: form });
      const data = await res.json();

      removePendingEntry(i);

      if (!res.ok) {
        showNotification(data.error || 'Processing failed', 'error');
        continue;
      }

      for (const item of data.results) {
        if (item.type === 'text') {
          sourceText.value += (sourceText.value ? '\n\n' : '') + item.text;
          hasText = true;
        } else if (item.type === 'document') {
          appendDocResult(item);
        }
      }
    } catch (e) {
      removePendingEntry(i);
      showNotification('Network error', 'error');
    }
  }

  sourceText.disabled = false;
  if (hasText) {
    updateCharCount();
    translate(true);
  }
}

// ── Event listeners ───────────────────────────────────────────────────────
translateBtn.addEventListener('click', () => translate());

clearBtn.addEventListener('click', () => {
  sourceText.value = '';
  lastTranslatedText = '';
  setOutput('');
  updateCharCount();
  detectedBadge.classList.add('hidden');
  chunkProgress.classList.add('hidden');
  copyBtn.classList.add('hidden');
  renderResultsDocList([]);
  transcribeStatus.innerHTML = '';
  transcribeStatus.classList.add('hidden');
});

copyBtn.addEventListener('click', async () => {
  const text = outputDiv.innerText;
  if (!text) return;
  await navigator.clipboard.writeText(text);
  showNotification('Copied', 'success');
});

swapBtn.addEventListener('click', () => {
  const srcCode = sourceLangSel.value;
  const tgtCode = targetLangSel.value;

  if (srcCode === 'auto') {
    const tgt = outputDiv.innerText;
    if (tgt && tgt !== 'Translation will appear here\u2026') {
      sourceText.value = tgt;
      setOutput('');
      updateCharCount();
      if (Array.from(sourceLangSel.options).some(o => o.value === tgtCode)) {
        sourceLangSel.value = tgtCode;
      }
    }
    return;
  }

  if (Array.from(targetLangSel.options).some(o => o.value === srcCode)) targetLangSel.value = srcCode;
  if (Array.from(sourceLangSel.options).some(o => o.value === tgtCode)) sourceLangSel.value = tgtCode;

  const curTgt = outputDiv.innerText;
  if (curTgt && curTgt !== 'Translation will appear here…') {
    sourceText.value = curTgt;
    lastTranslatedText = curTgt;
    translate(true);
  }
});

fileInput.addEventListener('change', () => {
  if (fileInput.files.length > 0) handleFiles(fileInput.files);
  fileInput.value = '';
});

// Drag and drop
window.addEventListener('dragover', e => {
  e.preventDefault();
  dropOverlay.classList.remove('hidden');
});
dropOverlay.addEventListener('dragleave', () => dropOverlay.classList.add('hidden'));
dropOverlay.addEventListener('drop', e => {
  e.preventDefault();
  dropOverlay.classList.add('hidden');
  const files = e.dataTransfer?.files;
  if (files && files.length > 0) handleFiles(files);
});

// ── Helpers ───────────────────────────────────────────────────────────────
function renderResultsDocList(files) {
  if (!files || files.length === 0) {
    resultsDocList.classList.add('hidden');
    resultsDocList.innerHTML = '';
    return;
  }

  resultsDocList.classList.remove('hidden');
  resultsDocList.innerHTML = '<div class="doc-list-title">Translated Documents</div>';

  const ul = document.createElement('ul');
  ul.className = 'doc-file-list';
  files.forEach(f => {
    const li = document.createElement('li');
    li.textContent = f.filename;
    const dl = document.createElement('button');
    dl.className = 'btn-primary doc-dl-btn';
    dl.style.fontSize = '0.75rem';
    dl.style.padding = '4px 8px';
    dl.textContent = 'Download';
    dl.onclick = () => downloadBase64File(f.data, f.filename, f.mime);
    li.appendChild(dl);
    ul.appendChild(li);
  });
  resultsDocList.appendChild(ul);
}

function downloadBase64File(b64, filename, mime) {
  const binary = atob(b64);
  const arr    = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) arr[i] = binary.charCodeAt(i);
  const blob = new Blob([arr], { type: mime });
  const url  = URL.createObjectURL(blob);
  const a    = document.createElement('a');
  a.href     = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

function updateCharCount() {
  charCount.textContent = sourceText.value.length.toLocaleString();
}

function setOutput(text) {
  outputDiv.classList.remove('loading');
  if (!text) {
    outputDiv.innerHTML = '<span class="placeholder">Translation will appear here…</span>';
    copyBtn.classList.add('hidden');
  } else {
    outputDiv.textContent = text;
    copyBtn.classList.remove('hidden');
  }
}

function setOutputLoading(loading) {
  if (loading) {
    outputDiv.classList.add('loading');
  } else {
    outputDiv.classList.remove('loading');
  }
}

function showPendingQueue(files) {
  transcribeStatus.innerHTML = '';
  files.forEach((file, i) => {
    const row = document.createElement('div');
    row.className = 'pending-file';
    row.dataset.pendingIndex = i;
    row.innerHTML = `<span class="spinner"></span><span class="pending-file-name">${file.name}</span>`;
    transcribeStatus.appendChild(row);
  });
  transcribeStatus.classList.remove('hidden');
}

function removePendingEntry(index) {
  const row = transcribeStatus.querySelector(`[data-pending-index="${index}"]`);
  if (row) row.remove();
  if (!transcribeStatus.querySelector('.pending-file')) {
    transcribeStatus.classList.add('hidden');
  }
}

function appendDocResult(item) {
  if (resultsDocList.classList.contains('hidden') || !resultsDocList.querySelector('.doc-file-list')) {
    resultsDocList.innerHTML = '<div class="doc-list-title">Translated Documents</div><ul class="doc-file-list"></ul>';
    resultsDocList.classList.remove('hidden');
  }
  const ul = resultsDocList.querySelector('.doc-file-list');
  const li = document.createElement('li');
  li.textContent = item.filename;
  const btn = document.createElement('button');
  btn.className = 'btn-primary doc-dl-btn';
  btn.textContent = 'Download';
  btn.onclick = () => downloadBase64File(item.data, item.filename, item.mime);
  li.appendChild(btn);
  ul.appendChild(li);
}

// ── Bitvault integration ──────────────────────────────────────────────────
async function saveToBitvault(text, tab) {
  try {
    const res = await fetch('/api/save-to-bitvault', {
      method:  'POST',
      headers: { 'Content-Type': 'application/json' },
      body:    JSON.stringify({ text }),
    });
    const bodyText = await res.text();
    let data = null;
    try { data = JSON.parse(bodyText); } catch (_) { /* non-JSON response */ }
    if (res.ok) {
      if (data && data.url) {
        if (tab && !tab.closed) {
          tab.location.href = data.url;
        } else {
          window.location.href = data.url;
        }
        showNotification('Saved to Bitvault', 'success');
      } else {
        if (tab && !tab.closed) tab.close();
        showNotification('Save succeeded but no URL returned', 'error');
      }
    } else {
      if (tab && !tab.closed) tab.close();
      showNotification((data && data.error) || bodyText || 'Save failed', 'error');
    }
  } catch (_) {
    if (tab && !tab.closed) tab.close();
    showNotification('Network error', 'error');
  }
}

saveSrcBtn.addEventListener('click', () => {
  const text = sourceText.value.trim();
  if (!text) return;
  const tab = window.open('', '_blank');
  if (!tab) { showNotification('Allow popups to save to Bitvault', 'error'); return; }
  saveToBitvault(text, tab);
});

saveOutBtn.addEventListener('click', () => {
  const text = (lastTranslatedText || '').trim();
  if (!text) return;
  const tab = window.open('', '_blank');
  if (!tab) { showNotification('Allow popups to save to Bitvault', 'error'); return; }
  saveToBitvault(text, tab);
});

let notifTimer = null;
function showNotification(msg, type = '') {
  notification.textContent = msg;
  notification.className   = 'notification' + (type ? ' ' + type : '');
  notification.classList.remove('hidden');
  clearTimeout(notifTimer);
  notifTimer = setTimeout(() => notification.classList.add('hidden'), type === 'error' ? 5000 : 3000);
}

init();
