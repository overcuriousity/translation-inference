'use strict';

// ── State ────────────────────────────────────────────────────────────────
let availableLanguages = [];
let availableModels    = [];
let userEndpoint       = '';
let userApiKey         = '';
let userTtsEndpoint    = '';
let userTtsApiKey      = '';
let lastTranslatedText = '';
let lastOutputText     = '';     // actual translated output (differs from source)
let ttsObjectUrl       = null;   // current blob: URL for the active TTS audio
let mediaRecorder      = null;
let ttsAudio           = null;   // active Audio instance
let serverTtsConfigured = false;
let serverTtsLanguages  = [];   // language codes with a configured TTS voice
let detectedSourceLang    = null;  // language code like 'en', null = not yet detected
let lastDetectedTextLength = 0;
let lastDetectedTextSnippet = '';
let detectionTimer        = null;
let detectionRequestId    = 0;    // incremented per request; guards stale async responses
let isPasting             = false;
let srcTtsAudio           = null;
let srcTtsObjectUrl       = null;

// ── DOM refs ─────────────────────────────────────────────────────────────
const targetLangSel    = document.getElementById('target-lang');
const outputFormatSel  = document.getElementById('output-format-select');
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
const uploadLabel      = document.getElementById('upload-label');
const resultsDocList    = document.getElementById('results-doc-list');
const transcribeStatus = document.getElementById('transcribe-status');
const chunkProgress    = document.getElementById('chunk-progress');
const notification     = document.getElementById('notification');
const configPanel      = document.getElementById('config-panel');
const configEndpoint   = document.getElementById('config-endpoint');
const configApikey     = document.getElementById('config-apikey');
const configConnectBtn = document.getElementById('config-connect-btn');
const configMsg        = document.getElementById('config-msg');
const gatedRow         = document.getElementById('gated-row');
const configAccesskey  = document.getElementById('config-accesskey');
const configGatedBtn   = document.getElementById('config-gated-btn');
const gatedMsg         = document.getElementById('gated-msg');
const configSeparator  = document.getElementById('config-separator');
const settingsBtn      = document.getElementById('settings-btn');
const dropOverlay      = document.getElementById('drop-overlay');
const voiceBtn         = document.getElementById('voice-btn');
const sourcePanel      = document.querySelector('.panel-source');
const saveSrcBtn           = document.getElementById('save-src-btn');
const saveOutBtn           = document.getElementById('save-out-btn');
const ttsBtn               = document.getElementById('tts-btn');
const configTtsEndpoint    = document.getElementById('config-tts-endpoint');
const configTtsApikey      = document.getElementById('config-tts-apikey');
const configTtsBtn         = document.getElementById('config-tts-btn');
const ttsMsg               = document.getElementById('tts-msg');
const srcTtsBtn            = document.getElementById('src-tts-btn');

// ── Boot ─────────────────────────────────────────────────────────────────
async function init() {
  const status = await fetch('/api/status').then(r => r.json())
    .catch(() => ({ server_configured: false, gated_configured: false, session_active: false, bitvault_configured: false, tts_configured: false }));

  if (status.bitvault_configured) {
    saveSrcBtn.classList.remove('hidden');
    saveOutBtn.classList.remove('hidden');
  }

  if (status.gated_configured) {
    gatedRow.classList.remove('hidden');
    configSeparator.classList.remove('hidden');
  }

  serverTtsConfigured = !!status.tts_configured;
  serverTtsLanguages  = status.tts_languages || [];
  updateTtsButtonVisibility();
  updateSrcTtsButtonVisibility();

  const hasAccess = status.server_configured || status.session_active;
  if (!hasAccess) {
    const msg = status.gated_configured
      ? 'Enter your access key or configure your own endpoint.'
      : 'Please configure your API credentials.';
    showConfigPanel(msg);
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
        detectLanguage(sourceText.value);
        if (hasAccess) {
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

configGatedBtn.addEventListener('click', async () => {
  const key = configAccesskey.value.trim();
  if (!key) {
    gatedMsg.textContent = 'Access key required.';
    gatedMsg.className = 'config-msg error';
    return;
  }

  configGatedBtn.disabled = true;
  gatedMsg.textContent = 'Verifying\u2026';
  gatedMsg.className = 'config-msg';

  try {
    const res  = await fetch('/api/config/gated', {
      method:  'POST',
      headers: { 'Content-Type': 'application/json' },
      body:    JSON.stringify({ access_key: key }),
    });
    const data = await res.json();

    if (res.ok) {
      userEndpoint = '';
      userApiKey   = '';
      configAccesskey.value = '';
      gatedMsg.textContent = '\u2713 Access granted';
      gatedMsg.className = 'config-msg success';
      await Promise.all([loadLanguages(), loadModels()]);
      setTimeout(() => configPanel.classList.add('hidden'), 800);
    } else {
      gatedMsg.textContent = data.error || 'Invalid access key';
      gatedMsg.className = 'config-msg error';
    }
  } catch (_) {
    gatedMsg.textContent = 'Network error';
    gatedMsg.className = 'config-msg error';
  } finally {
    configGatedBtn.disabled = false;
  }
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

    targetLangSel.innerHTML = '';
    for (const lang of availableLanguages.filter(l => l.code !== 'auto')) {
      const opt = document.createElement('option');
      opt.value = lang.code;
      opt.textContent = serverTtsLanguages.includes(lang.code)
        ? lang.name + ' 🔊'
        : lang.name;
      targetLangSel.appendChild(opt);
    }

    // Auto-set target language from browser preferences, fallback to English
    const browserLangs = navigator.languages && navigator.languages.length
      ? navigator.languages : [navigator.language || 'en'];
    let targetSet = false;
    for (const bl of browserLangs) {
      const code = bl.split('-')[0].toLowerCase();
      const match = availableLanguages.find(l => l.code === code && l.code !== 'auto');
      if (match) {
        targetLangSel.value = match.code;
        targetSet = true;
        break;
      }
    }
    if (!targetSet) targetLangSel.value = 'en';
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
  lastOutputText = '';
  updateTtsButtonVisibility();

  try {
    const body = {
      text,
      source_lang: 'auto',
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
      if (!isAuto) {
        const errBody = await res.json().catch(() => null);
        showNotification((errBody && errBody.error) || 'Translation error', 'error');
      }
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
      lastOutputText = partial;
      if (!isAuto) showNotification(errMsg || 'Translation error', 'error');
    } else {
      lastOutputText = textSoFar;
    }

    chunkProgress.classList.add('hidden');
    updateTtsButtonVisibility();

  } catch (e) {
    if (!isAuto) showNotification('Network error', 'error');
  } finally {
    setOutputLoading(false);
    chunkProgress.classList.add('hidden');
  }
}

function targetLangName(code) {
  const found = availableLanguages.find(l => l.code === code);
  return found ? found.name : code;
}

// ── Input ─────────────────────────────────────────────────────────────────
sourceText.addEventListener('input', () => {
  updateCharCount();
  if (isPasting) return;
  const text = sourceText.value;
  if (!text.trim()) {
    resetDetectedLang();
  } else {
    scheduleDetection(text);
  }
});

sourceText.addEventListener('paste', () => {
  isPasting = true;
  setTimeout(() => {
    isPasting = false;
    const text = sourceText.value;
    if (text.trim()) {
      // Reset stale detection state before starting a fresh request.
      detectedSourceLang = null;
      detectedBadge.classList.add('hidden');
      updateSrcTtsButtonVisibility();
      clearTimeout(detectionTimer);
      detectLanguage(text);
    } else {
      resetDetectedLang();
    }
  }, 0);
});

sourceText.addEventListener('keydown', e => {
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    translate();
  }
});

targetLangSel.addEventListener('change', () => { lastTranslatedText = ''; lastOutputText = ''; translate(true); });
modelSel.addEventListener('change', () => { lastTranslatedText = ''; lastOutputText = ''; translate(true); });

// ── Transcription & Upload ────────────────────────────────────────────────
function setTranscribeBusy(busy) {
  uploadLabel.classList.toggle('transcribing', busy);
  voiceBtn.classList.toggle('transcribing', busy);
}

async function handleFiles(files) {
  if (!files || files.length === 0) return;
  prepareOutputFormatForFiles(files);
  const fileArray = Array.from(files);
  sourceText.value = '';
  sourceText.disabled = true;
  setTranscribeBusy(true);
  showPendingQueue(fileArray);

  let hasText = false;

  try {
    for (let i = 0; i < fileArray.length; i++) {
      const file = fileArray[i];
      try {
        const form = new FormData();
        form.append('file', file, file.name);
        form.append('source_lang', 'auto');
        form.append('target_lang', targetLangName(targetLangSel.value));
        form.append('model', modelSel.value || '');
        form.append('whisper_model', whisperModelSel.value || '');
        const fileExt = file.name.split('.').pop().toLowerCase();
        if (['pdf', 'docx', 'odt'].includes(fileExt) && !outputFormatSel.classList.contains('hidden')) {
          form.append('output_format', outputFormatSel.value);
        }
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
  } finally {
    setTranscribeBusy(false);
    sourceText.disabled = false;
  }

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
  lastOutputText = '';
  setOutput('');
  updateCharCount();
  resetDetectedLang();
  chunkProgress.classList.add('hidden');
  copyBtn.classList.add('hidden');
  ttsBtn.classList.add('hidden');
  stopTts();
  outputFormatSel.classList.add('hidden');
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
  const tgt = outputDiv.innerText;
  if (tgt && tgt !== 'Translation will appear here\u2026') {
    sourceText.value = tgt;
    setOutput('');
    updateCharCount();
    resetDetectedLang();
    scheduleDetection(tgt);
  }
});

function prepareOutputFormatForFiles(files) {
  if (!files || files.length === 0) return;
  const allowedExts = ['pdf', 'docx', 'odt'];
  const exts = Array.from(files).map(f => {
    const parts = f.name.split('.');
    return parts.length < 2 ? '' : parts.pop().toLowerCase();
  });
  if (!exts.every(e => allowedExts.includes(e))) {
    outputFormatSel.classList.add('hidden');
    return;
  }
  const uniqueExts = [...new Set(exts)];
  if (uniqueExts.length === 1) {
    outputFormatSel.value = uniqueExts[0] === 'pdf' ? 'pdf' : 'odt';
  }
  // mixed supported types: keep current selector value, still show it
  outputFormatSel.classList.remove('hidden');
}

fileInput.addEventListener('change', () => {
  if (fileInput.files.length > 0) {
    handleFiles(fileInput.files);
  }
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
  if (files && files.length > 0) {
    handleFiles(files);
  }
});

// ── Voice input ───────────────────────────────────────────────────────────
voiceBtn.addEventListener('click', async () => {
  if (voiceBtn.classList.contains('transcribing')) return;

  if (mediaRecorder && mediaRecorder.state === 'recording') {
    mediaRecorder.stop();
    return;
  }

  if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === 'undefined') {
    showNotification('Audio recording not supported in this browser', 'error');
    return;
  }

  let stream = null;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    const chunks = [];
    const recorder = new MediaRecorder(stream);
    mediaRecorder = recorder;

    recorder.addEventListener('dataavailable', e => {
      if (e.data.size > 0) chunks.push(e.data);
    });

    recorder.addEventListener('stop', async () => {
      stream.getTracks().forEach(t => t.stop());
      voiceBtn.classList.remove('recording');
      voiceBtn.setAttribute('aria-pressed', 'false');
      voiceBtn.title = 'Record voice input';

      const mimeType = recorder.mimeType || 'audio/webm';
      const ext = mimeType.includes('ogg') ? 'ogg' : mimeType.includes('mp4') ? 'mp4' : 'webm';
      const blob = new Blob(chunks, { type: mimeType });
      if (blob.size === 0) {
        showNotification('Recording was empty — please try again', 'error');
        return;
      }
      const file = new File([blob], `recording.${ext}`, { type: mimeType });
      await handleFiles([file]);
    });

    recorder.start();
    voiceBtn.classList.add('recording');
    voiceBtn.setAttribute('aria-pressed', 'true');
    voiceBtn.title = 'Stop recording';
  } catch (e) {
    if (stream) stream.getTracks().forEach(t => t.stop());
    const msg = e.name === 'NotAllowedError' ? 'Microphone access denied'
              : e.name === 'NotFoundError'   ? 'No microphone found'
              : 'Could not start recording';
    showNotification(msg, 'error');
  }
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
    stopTts();
    updateTtsButtonVisibility();
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
      const msg = (data && data.error) || bodyText || 'Save failed';
      showNotification(msg.length > 200 ? msg.slice(0, 200) + '…' : msg, 'error');
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
  tab.opener = null;
  saveToBitvault(text, tab);
});

saveOutBtn.addEventListener('click', () => {
  const text = (lastTranslatedText || '').trim();
  if (!text) return;
  const tab = window.open('', '_blank');
  if (!tab) { showNotification('Allow popups to save to Bitvault', 'error'); return; }
  tab.opener = null;
  saveToBitvault(text, tab);
});

// ── TTS ───────────────────────────────────────────────────────────────────
function updateTtsButtonVisibility() {
  const hasTts = !!userTtsEndpoint
    || (serverTtsConfigured && serverTtsLanguages.includes(targetLangSel.value));
  // Only show the button when there is translated text available.
  const hasText = lastOutputText.trim().length > 0;
  if (hasTts && hasText) {
    ttsBtn.classList.remove('hidden');
  } else {
    ttsBtn.classList.add('hidden');
    stopTts();
  }
}

function stopTts() {
  if (ttsAudio) {
    ttsAudio.pause();
    ttsAudio.src = '';
    ttsAudio = null;
  }
  if (ttsObjectUrl) {
    URL.revokeObjectURL(ttsObjectUrl);
    ttsObjectUrl = null;
  }
  ttsBtn.classList.remove('playing', 'loading');
  ttsBtn.setAttribute('aria-pressed', 'false');
  ttsBtn.title = 'Read aloud';
}

ttsBtn.addEventListener('click', async () => {
  // Ignore clicks while a TTS request is already in flight.
  if (ttsBtn.classList.contains('loading')) return;
  // Second click stops playback.
  if (ttsAudio && !ttsAudio.paused) {
    stopTts();
    return;
  }

  const text = lastOutputText.trim();
  if (!text) return;

  stopSrcTts();
  ttsBtn.classList.add('loading');
  ttsBtn.setAttribute('aria-pressed', 'true');
  ttsBtn.title = 'Loading audio\u2026';

  try {
    const body = { text, target_lang: targetLangSel.value };
    if (userTtsEndpoint && userTtsApiKey) {
      body.tts_endpoint = userTtsEndpoint;
      body.tts_api_key  = userTtsApiKey;
    }

    const res = await fetch('/api/tts', {
      method:  'POST',
      headers: { 'Content-Type': 'application/json' },
      body:    JSON.stringify(body),
    });

    if (!res.ok) {
      let msg = 'TTS failed';
      try { const d = await res.json(); msg = d.error || msg; } catch (_) {}
      showNotification(msg, 'error');
      ttsBtn.classList.remove('loading');
      ttsBtn.setAttribute('aria-pressed', 'false');
      ttsBtn.title = 'Read aloud';
      return;
    }

    const blob = await res.blob();
    stopTts(); // revoke any previous blob URL and clear audio before creating a new one
    ttsObjectUrl = URL.createObjectURL(blob);

    ttsAudio = new Audio(ttsObjectUrl);
    ttsAudio.addEventListener('ended', () => {
      stopTts();
    });
    ttsAudio.addEventListener('error', () => {
      if (!ttsAudio) return; // triggered by stopTts() setting src='', not a real error
      showNotification('Audio playback error', 'error');
      stopTts();
    });

    ttsBtn.classList.remove('loading');
    ttsBtn.classList.add('playing');
    ttsBtn.title = 'Stop';
    ttsAudio.play().catch(err => {
      if (err.name !== 'AbortError') {
        showNotification('Audio playback error', 'error');
        stopTts();
      }
    });
  } catch (e) {
    showNotification('Network error', 'error');
    ttsBtn.classList.remove('loading');
    ttsBtn.setAttribute('aria-pressed', 'false');
    ttsBtn.title = 'Read aloud';
  }
});

configTtsBtn.addEventListener('click', () => {
  const ep  = configTtsEndpoint.value.trim();
  const key = configTtsApikey.value.trim();

  if (ep && !key) {
    ttsMsg.textContent = 'TTS API key required.';
    ttsMsg.className = 'config-msg error';
    return;
  }

  userTtsEndpoint = ep;
  userTtsApiKey   = key;

  if (ep) {
    ttsMsg.textContent = '\u2713 TTS endpoint set';
    ttsMsg.className = 'config-msg success';
  } else {
    ttsMsg.textContent = 'TTS endpoint cleared';
    ttsMsg.className = 'config-msg';
  }

  updateTtsButtonVisibility();
  updateSrcTtsButtonVisibility();
  setTimeout(() => { ttsMsg.textContent = ''; }, 2000);
});

// ── Language detection ────────────────────────────────────────────────────
function resetDetectedLang() {
  detectedSourceLang = null;
  lastDetectedTextLength = 0;
  lastDetectedTextSnippet = '';
  clearTimeout(detectionTimer);
  detectionTimer = null;
  detectedBadge.classList.add('hidden');
  updateSrcTtsButtonVisibility();
}

function isSignificantTextChange(text) {
  if (lastDetectedTextLength === 0 && lastDetectedTextSnippet === '') return true;
  const lenDiff = Math.abs(text.length - lastDetectedTextLength);
  const threshold = Math.max(50, Math.floor(lastDetectedTextLength * 0.2));
  if (lenDiff > threshold) return true;
  return text.slice(0, 100) !== lastDetectedTextSnippet.slice(0, 100);
}

async function detectLanguage(text) {
  const reqId   = ++detectionRequestId;
  const snippet = text.slice(0, 500);
  if (snippet.trim().length < 10) {
    resetDetectedLang();
    return;
  }

  try {
    const body = { text: snippet, ...apiCredentials() };
    const res = await fetch('/api/detect-language', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!res.ok) return;

    // Discard response if a newer request has since been issued.
    if (reqId !== detectionRequestId) return;

    const data = await res.json();
    const code = data.language.trim();
    const lang = availableLanguages.find(l => l.code.toLowerCase() === code.toLowerCase() && l.code !== 'auto');
    if (lang) {
      detectedSourceLang = lang.code;
      lastDetectedTextLength = text.length;
      lastDetectedTextSnippet = text.slice(0, 500);
      detectedBadge.textContent = lang.name;
      detectedBadge.classList.remove('hidden');
      updateSrcTtsButtonVisibility();
    }
  } catch (_) {
    // Silently ignore detection errors
  }
}

function scheduleDetection(text) {
  if (text.trim().length < 10) {
    resetDetectedLang();
    return;
  }
  if (!isSignificantTextChange(text)) return;

  // Significant change: reset displayed detection, then re-detect
  detectedSourceLang = null;
  detectedBadge.classList.add('hidden');
  updateSrcTtsButtonVisibility();

  clearTimeout(detectionTimer);
  detectionTimer = setTimeout(() => detectLanguage(text), 500);
}

// ── Source TTS ────────────────────────────────────────────────────────────
function updateSrcTtsButtonVisibility() {
  const hasTts = !!userTtsEndpoint
    || (serverTtsConfigured && detectedSourceLang !== null && serverTtsLanguages.includes(detectedSourceLang));
  const hasText = sourceText.value.trim().length > 0;
  const hasLang = detectedSourceLang !== null;
  if (hasTts && hasText && hasLang) {
    srcTtsBtn.classList.remove('hidden');
  } else {
    srcTtsBtn.classList.add('hidden');
    stopSrcTts();
  }
}

function stopSrcTts() {
  if (srcTtsAudio) {
    srcTtsAudio.pause();
    srcTtsAudio.src = '';
    srcTtsAudio = null;
  }
  if (srcTtsObjectUrl) {
    URL.revokeObjectURL(srcTtsObjectUrl);
    srcTtsObjectUrl = null;
  }
  srcTtsBtn.classList.remove('playing', 'loading');
  srcTtsBtn.setAttribute('aria-pressed', 'false');
  srcTtsBtn.title = 'Read source aloud';
}

srcTtsBtn.addEventListener('click', async () => {
  if (srcTtsBtn.classList.contains('loading')) return;
  if (srcTtsAudio && !srcTtsAudio.paused) {
    stopSrcTts();
    return;
  }

  const text = sourceText.value.trim();
  if (!text) return;

  // Detect language on demand if not yet detected
  if (detectedSourceLang === null) {
    srcTtsBtn.classList.add('loading');
    srcTtsBtn.title = 'Detecting language\u2026';
    await detectLanguage(text);
    srcTtsBtn.classList.remove('loading');
    srcTtsBtn.title = 'Read source aloud';
    if (detectedSourceLang === null) {
      showNotification('Could not detect source language', 'error');
      return;
    }
  }

  stopTts();  // stop output TTS if playing
  srcTtsBtn.classList.add('loading');
  srcTtsBtn.setAttribute('aria-pressed', 'true');
  srcTtsBtn.title = 'Loading audio\u2026';

  try {
    const body = { text, target_lang: detectedSourceLang };
    if (userTtsEndpoint && userTtsApiKey) {
      body.tts_endpoint = userTtsEndpoint;
      body.tts_api_key  = userTtsApiKey;
    }

    const res = await fetch('/api/tts', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });

    if (!res.ok) {
      let msg = 'TTS failed';
      try { const d = await res.json(); msg = d.error || msg; } catch (_) {}
      showNotification(msg, 'error');
      srcTtsBtn.classList.remove('loading');
      srcTtsBtn.setAttribute('aria-pressed', 'false');
      srcTtsBtn.title = 'Read source aloud';
      return;
    }

    const blob = await res.blob();
    stopSrcTts();
    srcTtsObjectUrl = URL.createObjectURL(blob);

    srcTtsAudio = new Audio(srcTtsObjectUrl);
    srcTtsAudio.addEventListener('ended', () => stopSrcTts());
    srcTtsAudio.addEventListener('error', () => {
      if (!srcTtsAudio) return;
      showNotification('Audio playback error', 'error');
      stopSrcTts();
    });

    srcTtsBtn.classList.remove('loading');
    srcTtsBtn.classList.add('playing');
    srcTtsBtn.title = 'Stop';
    srcTtsAudio.play().catch(err => {
      if (err.name !== 'AbortError') {
        showNotification('Audio playback error', 'error');
        stopSrcTts();
      }
    });
  } catch (e) {
    showNotification('Network error', 'error');
    srcTtsBtn.classList.remove('loading');
    srcTtsBtn.setAttribute('aria-pressed', 'false');
    srcTtsBtn.title = 'Read source aloud';
  }
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
