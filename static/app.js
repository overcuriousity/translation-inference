'use strict';

// ── State ────────────────────────────────────────────────────────────────
let availableLanguages      = [];
let availableModels         = [];
let selectedTargetLangCode  = 'en';
let langPickerOpen          = false;
let userEndpoint            = '';
let userApiKey              = '';
let userTtsEndpoint    = '';
let userTtsApiKey      = '';
let lastTranslatedText = '';
let lastOutputText     = '';     // actual translated output (differs from source)
let ttsObjectUrl       = null;   // current blob: URL for the active TTS audio
let mediaRecorder      = null;
let ttsAudio           = null;   // active Audio instance
let serverTtsConfigured = false;
let serverTtsLanguages  = [];   // language codes with a configured TTS voice
let serverTtsHostname   = null; // TTS endpoint hostname for backend-info display
let sessionActive       = false; // true when a gated/byok session cookie exists
let sessionTier         = null;  // 'free' | 'gated' | 'byok' | null
let charLimit           = null;  // max input chars for this tier, null = unlimited
let detectedSourceLang    = null;  // language code like 'en', null = not yet detected
let lastDetectedTextLength = 0;
let lastDetectedTextSnippet = '';
let detectionTimer        = null;
let detectionRequestId    = 0;    // incremented per request; guards stale async responses
let isPasting             = false;
let srcTtsAudio           = null;
let srcTtsObjectUrl       = null;
let activeTab             = 'text';
let mdRenderActive        = false;
let paraViewActive        = false;
let convHistory           = [];  // [{speaker:'a'|'b', source, translation}]
let convLangA             = 'de';
let convLangB             = 'en';
let convRecording         = null;  // 'a' | 'b' | null
let convAutoTts           = true;
let convTtsAudio          = null;
let convTtsObjUrl         = null;
let convMediaRecorder     = null;

// ── DOM refs ─────────────────────────────────────────────────────────────
const backendInfoEl      = document.getElementById('backend-info');
const langPickerBtn      = document.getElementById('lang-picker-btn');
const langPickerLabel    = document.getElementById('lang-picker-label');
const langPickerDropdown = document.getElementById('lang-picker-dropdown');
const langSearchInput    = document.getElementById('lang-search');
const langListEl         = document.getElementById('lang-list');
const tabTextBtn         = document.getElementById('tab-text');
const sourceLangInfo     = document.getElementById('source-lang-info');
const outputPanel        = document.querySelector('.panel-output');
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
const incompleteBadge      = document.getElementById('incomplete-badge');
const mdToggleBtn          = document.getElementById('md-toggle-btn');
const paraToggleBtn        = document.getElementById('para-toggle-btn');
const paraOutput           = document.getElementById('para-output');
const contextHintRow       = document.getElementById('context-hint-row');
const contextHintInput     = document.getElementById('context-hint');
const contextHintToggle    = document.getElementById('context-hint-toggle');
const tabDocumentBtn       = document.getElementById('tab-document');
const docUploadArea        = document.getElementById('doc-upload-area');
const docFileInput         = document.getElementById('doc-file-input');
const resultsDocList       = document.getElementById('results-doc-list');
const tabConvBtn           = document.getElementById('tab-conversation');
const convPanelEl          = document.getElementById('conv-panel');
const convMicA             = document.getElementById('conv-mic-a');
const convMicB             = document.getElementById('conv-mic-b');
const convLangAsel         = document.getElementById('conv-lang-a');
const convLangBsel         = document.getElementById('conv-lang-b');
const convTranscriptA      = document.getElementById('conv-transcript-a');
const convTranscriptB      = document.getElementById('conv-transcript-b');
const convAutoTtsInput     = document.getElementById('conv-auto-tts');
const convClearBtn         = document.getElementById('conv-clear-btn');
const convExportBtn        = document.getElementById('conv-export-btn');
const sourcePanelFooter    = document.querySelector('.panel-source .panel-footer');

// ── Boot ─────────────────────────────────────────────────────────────────
async function init() {
  const status = await fetch('/api/status').then(r => r.json())
    .catch(() => ({ server_configured: false, gated_configured: false, session_active: false, bitvault_configured: false, tts_configured: false, git_commit: null }));

  // Populate footer commit link — only link when the value is a real short SHA.
  const commitEl = document.getElementById('footer-commit');
  const gitCommit = typeof status.git_commit === 'string' ? status.git_commit.trim() : '';
  const isRealCommit = /^[0-9a-f]{7,40}$/i.test(gitCommit);
  if (commitEl) {
    if (isRealCommit) {
      commitEl.textContent = gitCommit;
      commitEl.href = `https://github.com/overcuriousity/translation-inference/commit/${gitCommit}`;
    } else {
      commitEl.textContent = gitCommit || '—';
      commitEl.removeAttribute('href');
    }
  }

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
  serverTtsHostname   = status.tts_hostname || null;
  sessionTier         = status.session_tier || null;
  sessionActive       = sessionTier === 'gated' || sessionTier === 'byok';
  charLimit           = status.char_limit || null;
  updateTtsButtonVisibility();
  updateSrcTtsButtonVisibility();
  updateFileTabVisibility();
  updateCharCount();
  // Show context hint toggle in initial text mode
  contextHintToggle.classList.remove('hidden');

  const hasAccess = status.server_configured || status.session_active;
  if (!hasAccess) {
    const msg = status.gated_configured
      ? 'Enter your access key or configure your own endpoint.'
      : 'Please configure your API credentials.';
    showConfigPanel(msg);
  } else {
    await Promise.all([loadLanguages(), loadModels()]);
    initConvLangSelects();
  }

  // Restore persisted state
  if (localStorage.getItem('mdRender') === 'true') {
    mdRenderActive = true;
    mdToggleBtn.setAttribute('aria-pressed', 'true');
  }
  if (localStorage.getItem('paraView') === 'true') {
    paraViewActive = true;
    paraToggleBtn.setAttribute('aria-pressed', 'true');
    // Don't auto-load the view here — there's no translated output yet on boot.
  }
  if (localStorage.getItem('contextHint')) {
    contextHintInput.value = localStorage.getItem('contextHint');
  }
  if (localStorage.getItem('convLangA')) convLangA = localStorage.getItem('convLangA');
  if (localStorage.getItem('convLangB')) convLangB = localStorage.getItem('convLangB');
  if (localStorage.getItem('convAutoTts') === 'false') {
    convAutoTts = false;
    convAutoTtsInput.checked = false;
  }
  // Re-sync language selects now that persisted codes are loaded
  if (convLangAsel.options.length > 0) {
    if (convLangA) convLangAsel.value = convLangA;
    if (convLangB) convLangBsel.value = convLangB;
  }

  // Pre-fill source text from a Bitvault paste URL passed as ?from=
  const fromUrl = new URLSearchParams(window.location.search).get('from');
  if (fromUrl && status.bitvault_configured) {
    try {
      const res = await fetch(`/api/proxy-text?url=${encodeURIComponent(fromUrl)}`);
      if (res.ok) {
        sourceText.value = await res.text();
        updateCharCount();
        if (hasAccess) {
          detectLanguage(sourceText.value);
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

function updateFileTabVisibility() {
  if (sessionActive) {
    tabDocumentBtn.classList.remove('hidden');
  } else {
    tabDocumentBtn.classList.add('hidden');
    if (activeTab === 'file') switchTab('text');
  }
}

function updateBackendInfo() {
  if (!backendInfoEl) return;
  const translModel = modelSel.value || null;
  const sttModel    = whisperModelSel.value || null;

  function serviceLabel(model, endpoint) {
    if (!model) return null;
    if (endpoint) {
      try {
        const host = new URL(endpoint).hostname;
        return `${host} · ${model}`;
      } catch (_) { /* fall through */ }
    }
    return model;
  }

  const parts = [];
  const tl = serviceLabel(translModel, userEndpoint);
  if (tl) parts.push(`Translation: ${tl}`);
  const st = serviceLabel(sttModel, userEndpoint);
  if (st) parts.push(`STT: ${st}`);

  // TTS: prefer user-supplied endpoint, fall back to server hostname.
  const ttsHost = userTtsEndpoint
    ? (() => { try { return new URL(userTtsEndpoint).hostname; } catch (_) { return null; } })()
    : serverTtsHostname;
  if (ttsHost) parts.push(`TTS: ${ttsHost}`);

  backendInfoEl.textContent = parts.join('  ·  ');
}

function promptReauth() {
  sessionActive = false;
  sessionTier   = null;
  charLimit     = null;
  updateFileTabVisibility();
  updateCharCount();
  showConfigPanel('Session expired. Please re-authenticate.');
  showNotification('Session expired \u2014 please re-authenticate', 'error');
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
      sessionTier   = 'gated';
      sessionActive = true;
      charLimit     = (await fetch('/api/status').then(r => r.json()).catch(() => ({}))).char_limit || null;
      updateFileTabVisibility();
      updateCharCount();
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
      sessionTier   = 'byok';
      sessionActive = true;
      charLimit     = null; // BYOK is always unlimited
      updateFileTabVisibility();
      updateCharCount();
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

    // Auto-set target language from browser preferences, fallback to English
    const browserLangs = navigator.languages && navigator.languages.length
      ? navigator.languages : [navigator.language || 'en'];
    let targetCode = 'en';
    for (const bl of browserLangs) {
      const code = bl.split('-')[0].toLowerCase();
      const match = availableLanguages.find(l => l.code === code && l.code !== 'auto');
      if (match) { targetCode = match.code; break; }
    }
    setTargetLang(targetCode);
    initConvLangSelects();
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
    // Free-tier users see what the server uses but cannot change it.
    const isFree = sessionTier === 'free' || (!sessionTier && !sessionActive);
    modelSel.disabled = isFree;
    whisperModelSel.disabled = isFree;

    updateBackendInfo();
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
      target_lang: targetLangName(selectedTargetLangCode),
      model: modelSel.value || undefined,
      context: contextHintInput.value.trim() || undefined,
      ...apiCredentials(),
    };

    const res  = await fetch('/api/translate/stream', {
      method:  'POST',
      headers: { 'Content-Type': 'application/json' },
      body:    JSON.stringify(body),
    });

    if (!res.ok) {
      if (res.status === 401) {
        promptReauth();
      } else if (!isAuto) {
        const errBody = await res.json().catch(() => null);
        showNotification((errBody && errBody.error) || 'Translation error', 'error');
      }
      setOutputLoading(false);
      return;
    }

    setOutput('');
    incompleteBadge.classList.add('hidden');
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
      incompleteBadge.classList.remove('hidden');
      if (!isAuto) showNotification(errMsg || 'Translation error', 'error');
    } else {
      lastOutputText = textSoFar;
    }

    chunkProgress.classList.add('hidden');
    updateTtsButtonVisibility();

    // Apply markdown rendering if active (only after full translation)
    if (mdRenderActive && lastOutputText) applyMdRender();

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
      detectedBadge.textContent = 'Detecting\u2026';
      detectedBadge.classList.remove('hidden');
      detectedBadge.classList.add('detecting');
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

// target language changes are handled inside setTargetLang()
modelSel.addEventListener('change', () => { lastTranslatedText = ''; lastOutputText = ''; updateBackendInfo(); translate(true); });
whisperModelSel.addEventListener('change', () => updateBackendInfo());

// ── Transcription & Upload ────────────────────────────────────────────────
function setTranscribeBusy(busy) {
  uploadLabel.classList.toggle('transcribing', busy);
  voiceBtn.classList.toggle('transcribing', busy);
}

async function handleFiles(files) {
  if (!files || files.length === 0) return;
  const fileArray = Array.from(files);
  sourceText.value = '';
  resetDetectedLang();
  sourceText.disabled = true;
  setTranscribeBusy(true);
  showPendingQueue(fileArray);

  let hasText = false;

  try {
    for (let i = 0; i < fileArray.length; i++) {
      const file = fileArray[i];
      const fileExt = file.name.split('.').pop().toLowerCase();
      try {
        // Subtitle files go to the subtitle endpoint (SSE progress stream)
        if (['srt', 'vtt'].includes(fileExt)) {
          const form = new FormData();
          form.append('file', file, file.name);
          form.append('source_lang', 'auto');
          form.append('target_lang', targetLangName(selectedTargetLangCode));
          form.append('model', modelSel.value || '');
          appendCredentialsToForm(form);

          const res = await fetch('/api/translate-subtitle', { method: 'POST', body: form });

          if (!res.ok) {
            const data = await res.json().catch(() => ({}));
            removePendingEntry(i);
            if (res.status === 401) { promptReauth(); break; }
            showNotification(data.error || 'Subtitle translation failed', 'error');
            continue;
          }

          // Stream SSE events: progress updates followed by a final done/error event.
          const result = await readSubtitleSSE(res, i);
          removePendingEntry(i);
          if (result.error) {
            showNotification(result.error, 'error');
          } else if (result.data) {
            appendDocResult(result);
          }
          continue;
        }

        // Audio/video files go to the upload/transcription endpoint
        const form = new FormData();
        form.append('file', file, file.name);
        form.append('whisper_model', whisperModelSel.value || '');
        appendCredentialsToForm(form);

        const res  = await fetch('/api/upload', { method: 'POST', body: form });
        const data = await res.json();

        removePendingEntry(i);

        if (!res.ok) {
          if (res.status === 401) { promptReauth(); break; }
          showNotification(data.error || 'Processing failed', 'error');
          continue;
        }

        for (const item of data.results) {
          if (item.type === 'text') {
            sourceText.value += (sourceText.value ? '\n\n' : '') + item.text;
            hasText = true;
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
  if (activeTab === 'file') {
    resultsDocList.innerHTML = '<div class="doc-list-placeholder">Translated files will appear here\u2026</div>';
    resultsDocList.classList.remove('hidden');
    transcribeStatus.innerHTML = '';
    transcribeStatus.classList.add('hidden');
    return;
  }
  sourceText.value = '';
  lastTranslatedText = '';
  lastOutputText = '';
  setOutput('');
  updateCharCount();
  resetDetectedLang();
  chunkProgress.classList.add('hidden');
  copyBtn.classList.add('hidden');
  mdToggleBtn.classList.add('hidden');
  paraToggleBtn.classList.add('hidden');
  paraOutput.classList.add('hidden');
  outputDiv.classList.remove('hidden');
  paraViewActive = false;
  paraToggleBtn.setAttribute('aria-pressed', 'false');
  ttsBtn.classList.add('hidden');
  stopTts();
  transcribeStatus.innerHTML = '';
  transcribeStatus.classList.add('hidden');
});

copyBtn.addEventListener('click', async () => {
  let text;
  if (paraViewActive) {
    const cells = paraOutput.querySelectorAll('.para-cell-target');
    text = Array.from(cells).map(c => c.textContent).filter(t => t.trim()).join('\n\n');
  } else {
    text = lastOutputText;
  }
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

fileInput.addEventListener('change', () => {
  if (fileInput.files.length > 0) handleFiles(fileInput.files);
  fileInput.value = '';
});

docFileInput.addEventListener('change', () => {
  if (docFileInput.files.length > 0) handleFiles(docFileInput.files);
  docFileInput.value = '';
});

// Drag and drop
function clearDragState() {
  dropOverlay.classList.add('hidden');
}

window.addEventListener('dragover', e => {
  e.preventDefault();
  if (activeTab === 'file') {
    docUploadArea.classList.add('drag-over');
  } else if (activeTab === 'text') {
    dropOverlay.classList.remove('hidden');
  }
});
// Safety net: prevent browser navigating to file and clean up state when
// dragging leaves the window or drops on an unhandled area.
window.addEventListener('dragleave', e => { if (!e.relatedTarget) clearDragState(); });
window.addEventListener('drop', e => { e.preventDefault(); clearDragState(); });

dropOverlay.addEventListener('dragleave', () => dropOverlay.classList.add('hidden'));
dropOverlay.addEventListener('drop', e => {
  e.preventDefault();
  dropOverlay.classList.add('hidden');
  const files = e.dataTransfer?.files;
  if (!files || files.length === 0) return;
  const avFiles = Array.from(files).filter(f =>
    f.type.startsWith('audio/') || f.type.startsWith('video/') ||
    /\.(mp3|mp4|m4a|wav|ogg|webm|flac|aac|mkv|avi|mov|wmv)$/i.test(f.name)
  );
  if (avFiles.length > 0) handleFiles(avFiles);
  else showNotification('Switch to File tab to translate subtitle files', 'error');
});

docUploadArea.addEventListener('dragleave', e => {
  if (!docUploadArea.contains(e.relatedTarget)) docUploadArea.classList.remove('drag-over');
});
docUploadArea.addEventListener('drop', e => {
  e.preventDefault();
  docUploadArea.classList.remove('drag-over');
  const files = e.dataTransfer?.files;
  if (!files || files.length === 0) return;
  const docFiles = Array.from(files).filter(f => /\.(srt|vtt)$/i.test(f.name));
  if (docFiles.length > 0) handleFiles(docFiles);
  else showNotification('Please drop SRT or VTT subtitle files', 'error');
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
      voiceBtn.classList.add('transcribing');
      voiceBtn.title = 'Transcribing\u2026';

      const mimeType = recorder.mimeType || 'audio/webm';
      const ext = mimeType.includes('ogg') ? 'ogg' : mimeType.includes('mp4') ? 'mp4' : 'webm';
      const blob = new Blob(chunks, { type: mimeType });
      if (blob.size === 0) {
        voiceBtn.classList.remove('transcribing');
        voiceBtn.title = 'Record voice input';
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
function updateCharCount() {
  const len = sourceText.value.length;
  charCount.textContent = charLimit
    ? `${len.toLocaleString()} / ${charLimit.toLocaleString()}`
    : len.toLocaleString();
  const over = charLimit && len > charLimit;
  charCount.classList.toggle('over-limit', !!over);
  translateBtn.disabled = !!over;
}

function setOutput(text) {
  outputDiv.classList.remove('loading');
  if (!text) {
    outputDiv.innerHTML = '<span class="placeholder">Translation will appear here\u2026</span>';
    copyBtn.classList.add('hidden');
    mdToggleBtn.classList.add('hidden');
    paraToggleBtn.classList.add('hidden');
    incompleteBadge.classList.add('hidden');
    stopTts();
    updateTtsButtonVisibility();
  } else {
    outputDiv.textContent = text;
    copyBtn.classList.remove('hidden');
    mdToggleBtn.classList.remove('hidden');
    paraToggleBtn.classList.remove('hidden');
  }
}

function applyMdRender() {
  outputDiv.innerHTML = DOMPurify.sanitize(marked.parse(lastOutputText));
}

function removeMdRender() {
  outputDiv.textContent = lastOutputText;
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
    const nameSpan = document.createElement('span');
    nameSpan.className = 'pending-file-name';
    nameSpan.dataset.filename = file.name;
    nameSpan.textContent = file.name;
    row.appendChild(document.createElement('span')).className = 'spinner';
    row.appendChild(nameSpan);
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

// Read the SSE stream from /api/translate-subtitle, updating the pending row
// with live cue progress. Returns {filename, data, mime} on success or
// {error} on failure.
async function readSubtitleSSE(res, pendingIndex) {
  const row = transcribeStatus.querySelector(`[data-pending-index="${pendingIndex}"]`);
  const progressLabel = row ? row.querySelector('.pending-file-name') : null;

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = '';

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });

    // SSE frames are separated by double newlines.
    const frames = buf.split('\n\n');
    buf = frames.pop(); // keep incomplete trailing frame

    for (const frame of frames) {
      if (!frame.trim()) continue;
      let eventType = 'message';
      let eventData = '';
      for (const line of frame.split('\n')) {
        if (line.startsWith('event:')) eventType = line.slice(6).trim();
        else if (line.startsWith('data:')) eventData = line.slice(5).trim();
      }
      if (!eventData) continue;

      let parsed;
      try { parsed = JSON.parse(eventData); } catch { continue; }

      if (eventType === 'progress' && progressLabel) {
        progressLabel.textContent = `${progressLabel.dataset.filename || ''} — cue ${parsed.done}/${parsed.total}`;
      } else if (eventType === 'done') {
        return parsed;
      } else if (eventType === 'error') {
        return { error: parsed.error || 'Subtitle translation failed' };
      }
    }
  }
  return { error: 'Subtitle translation ended unexpectedly' };
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

function downloadBase64File(b64, filename, mime) {
  const bytes = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
  const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
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
  const hasTts = (sessionActive && !!userTtsEndpoint)
    || (serverTtsConfigured && serverTtsLanguages.includes(selectedTargetLangCode));
  const hasText = lastOutputText.trim().length > 0;
  if (!hasTts) {
    ttsBtn.classList.add('hidden');
    stopTts();
  } else if (hasText) {
    ttsBtn.classList.remove('hidden');
    ttsBtn.removeAttribute('aria-disabled');
    ttsBtn.title = 'Read aloud';
  } else {
    ttsBtn.classList.remove('hidden');
    ttsBtn.setAttribute('aria-disabled', 'true');
    ttsBtn.title = 'Translate text first';
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
  if (ttsBtn.getAttribute('aria-disabled') !== 'true') ttsBtn.title = 'Read aloud';
}

ttsBtn.addEventListener('click', async () => {
  if (ttsBtn.getAttribute('aria-disabled') === 'true') return;
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
    const body = { text, target_lang: selectedTargetLangCode };
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
      if (res.status === 401) {
        promptReauth();
      } else {
        let msg = 'TTS failed';
        try { const d = await res.json(); msg = d.error || msg; } catch (_) {}
        showNotification(msg, 'error');
      }
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
  updateBackendInfo();

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
  detectionRequestId++;    // discard any in-flight request
  clearTimeout(detectionTimer);
  detectionTimer = null;
  detectedBadge.classList.add('hidden');
  detectedBadge.classList.remove('detecting');
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
    if (!res.ok) {
      if (reqId === detectionRequestId) {
        detectedBadge.classList.add('hidden');
        detectedBadge.classList.remove('detecting');
      }
      return;
    }

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
      detectedBadge.classList.remove('hidden', 'detecting');
      updateSrcTtsButtonVisibility();
    } else {
      detectedBadge.classList.add('hidden');
      detectedBadge.classList.remove('detecting');
    }
  } catch (_) {
    if (reqId === detectionRequestId) {
      detectedBadge.classList.add('hidden');
      detectedBadge.classList.remove('detecting');
    }
  }
}

function scheduleDetection(text) {
  if (text.trim().length < 10) {
    resetDetectedLang();
    return;
  }
  if (!isSignificantTextChange(text)) return;

  // Significant change: reset displayed detection and invalidate any
  // in-flight request before scheduling the new one.
  detectedSourceLang = null;
  detectedBadge.textContent = 'Detecting\u2026';
  detectedBadge.classList.remove('hidden');
  detectedBadge.classList.add('detecting');
  updateSrcTtsButtonVisibility();
  detectionRequestId++;

  clearTimeout(detectionTimer);
  detectionTimer = setTimeout(() => detectLanguage(text), 500);
}

// ── Source TTS ────────────────────────────────────────────────────────────
function updateSrcTtsButtonVisibility() {
  const anyTts = (sessionActive && !!userTtsEndpoint) || serverTtsConfigured;
  const hasTts = (sessionActive && !!userTtsEndpoint)
    || (serverTtsConfigured && detectedSourceLang !== null && serverTtsLanguages.includes(detectedSourceLang));
  const hasText = sourceText.value.trim().length > 0;
  const hasLang = detectedSourceLang !== null;
  if (!anyTts || !hasText) {
    srcTtsBtn.classList.add('hidden');
    stopSrcTts();
  } else if (!hasLang) {
    srcTtsBtn.classList.remove('hidden');
    srcTtsBtn.setAttribute('aria-disabled', 'true');
    srcTtsBtn.title = 'Detecting language\u2026';
    stopSrcTts();
  } else if (!hasTts) {
    srcTtsBtn.classList.remove('hidden');
    srcTtsBtn.setAttribute('aria-disabled', 'true');
    srcTtsBtn.title = 'TTS not available for this language';
    stopSrcTts();
  } else {
    srcTtsBtn.classList.remove('hidden');
    srcTtsBtn.removeAttribute('aria-disabled');
    srcTtsBtn.title = 'Read source aloud';
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
  if (srcTtsBtn.getAttribute('aria-disabled') !== 'true') srcTtsBtn.title = 'Read source aloud';
}

srcTtsBtn.addEventListener('click', async () => {
  if (srcTtsBtn.getAttribute('aria-disabled') === 'true') return;
  if (srcTtsBtn.classList.contains('loading')) return;
  if (srcTtsAudio && !srcTtsAudio.paused) {
    stopSrcTts();
    return;
  }

  const text = sourceText.value.trim();
  if (!text || detectedSourceLang === null) return;

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
      if (res.status === 401) {
        promptReauth();
      } else {
        let msg = 'TTS failed';
        try { const d = await res.json(); msg = d.error || msg; } catch (_) {}
        showNotification(msg, 'error');
      }
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

// ── Markdown rendering ────────────────────────────────────────────────────
mdToggleBtn.addEventListener('click', () => {
  if (!lastOutputText) return;
  mdRenderActive = !mdRenderActive;
  mdToggleBtn.setAttribute('aria-pressed', String(mdRenderActive));
  if (mdRenderActive) {
    applyMdRender();
  } else {
    removeMdRender();
  }
  localStorage.setItem('mdRender', String(mdRenderActive));
});

// ── Paragraph view ────────────────────────────────────────────────────────
paraToggleBtn.addEventListener('click', async () => {
  if (paraToggleBtn.getAttribute('aria-disabled') === 'true') return;
  paraViewActive = !paraViewActive;
  paraToggleBtn.setAttribute('aria-pressed', String(paraViewActive));
  localStorage.setItem('paraView', String(paraViewActive));

  if (paraViewActive) {
    await loadParaView();
  } else {
    paraOutput.classList.add('hidden');
    outputDiv.classList.remove('hidden');
    if (mdRenderActive && lastOutputText) applyMdRender();
  }
});

async function loadParaView() {
  const text = sourceText.value;
  if (!text.trim()) {
    paraViewActive = false;
    paraToggleBtn.setAttribute('aria-pressed', 'false');
    return;
  }

  const body = {
    text,
    source_lang: 'auto',
    target_lang: targetLangName(selectedTargetLangCode),
    model: modelSel.value || undefined,
    context: contextHintInput.value.trim() || undefined,
    ...apiCredentials(),
  };

  paraToggleBtn.setAttribute('aria-disabled', 'true');
  try {
    const res = await fetch('/api/translate/paragraphs', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => null);
      showNotification((err && err.error) || 'Paragraph view failed', 'error');
      paraViewActive = false;
      paraToggleBtn.setAttribute('aria-pressed', 'false');
      return;
    }
    const data = await res.json();
    renderParaView(data.paragraphs);
    outputDiv.classList.add('hidden');
    paraOutput.classList.remove('hidden');
  } catch (e) {
    showNotification('Network error', 'error');
    paraViewActive = false;
    paraToggleBtn.setAttribute('aria-pressed', 'false');
  } finally {
    paraToggleBtn.removeAttribute('aria-disabled');
  }
}

function renderParaView(paragraphs) {
  paraOutput.innerHTML = '';
  for (const pair of paragraphs) {
    if (!pair.source.trim() && !pair.translation.trim()) {
      const spacer = document.createElement('div');
      spacer.className = 'para-spacer';
      paraOutput.appendChild(spacer);
      continue;
    }
    const srcCell = document.createElement('div');
    srcCell.className = 'para-cell para-cell-source';
    srcCell.textContent = pair.source;
    const tgtCell = document.createElement('div');
    tgtCell.className = 'para-cell para-cell-target';
    tgtCell.textContent = pair.translation;
    paraOutput.appendChild(srcCell);
    paraOutput.appendChild(tgtCell);
  }
}

// ── Context hint ──────────────────────────────────────────────────────────
contextHintToggle.addEventListener('click', () => {
  const isOpen = !contextHintRow.classList.contains('hidden');
  contextHintRow.classList.toggle('hidden', isOpen);
  contextHintToggle.setAttribute('aria-pressed', String(!isOpen));
  if (!isOpen) contextHintInput.focus();
});

contextHintInput.addEventListener('input', () => {
  localStorage.setItem('contextHint', contextHintInput.value);
});

// ── Conversation mode ─────────────────────────────────────────────────────
function initConvLangSelects() {
  [convLangAsel, convLangBsel].forEach((sel, idx) => {
    sel.innerHTML = '';
    for (const lang of availableLanguages.filter(l => l.code !== 'auto')) {
      const opt = document.createElement('option');
      opt.value = lang.code;
      opt.textContent = lang.name;
      sel.appendChild(opt);
    }
    sel.value = idx === 0 ? convLangA : convLangB;
  });
}

convLangAsel.addEventListener('change', () => {
  convLangA = convLangAsel.value;
  localStorage.setItem('convLangA', convLangA);
});

convLangBsel.addEventListener('change', () => {
  convLangB = convLangBsel.value;
  localStorage.setItem('convLangB', convLangB);
});

convAutoTtsInput.addEventListener('change', () => {
  convAutoTts = convAutoTtsInput.checked;
  localStorage.setItem('convAutoTts', String(convAutoTts));
});

async function startConvRecording(speaker) {
  if (convRecording !== null) return;
  if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === 'undefined') {
    showNotification('Audio recording not supported in this browser', 'error');
    return;
  }

  const micBtn = speaker === 'a' ? convMicA : convMicB;
  let stream = null;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    const chunks = [];
    const recorder = new MediaRecorder(stream);
    convMediaRecorder = recorder;
    convRecording = speaker;
    micBtn.classList.add('recording');
    micBtn.setAttribute('aria-pressed', 'true');

    recorder.addEventListener('dataavailable', e => {
      if (e.data.size > 0) chunks.push(e.data);
    });

    recorder.addEventListener('stop', async () => {
      convMediaRecorder = null;
      stream.getTracks().forEach(t => t.stop());
      micBtn.classList.remove('recording');
      micBtn.classList.add('transcribing');

      const mimeType = recorder.mimeType || 'audio/webm';
      const ext = mimeType.includes('ogg') ? 'ogg' : mimeType.includes('mp4') ? 'mp4' : 'webm';
      const blob = new Blob(chunks, { type: mimeType });
      if (blob.size === 0) {
        micBtn.classList.remove('transcribing');
        micBtn.setAttribute('aria-pressed', 'false');
        convRecording = null;
        showNotification('Recording was empty \u2014 please try again', 'error');
        return;
      }
      const file = new File([blob], `recording.${ext}`, { type: mimeType });

      // Transcribe
      try {
        const form = new FormData();
        form.append('file', file, file.name);
        form.append('whisper_model', whisperModelSel.value || '');
        appendCredentialsToForm(form);
        const trRes = await fetch('/api/upload', { method: 'POST', body: form });
        if (!trRes.ok) {
          showNotification('Transcription failed', 'error');
          micBtn.classList.remove('transcribing');
          micBtn.setAttribute('aria-pressed', 'false');
          convRecording = null;
          return;
        }
        const trData = await trRes.json();
        const transcribed = trData.results.filter(r => r.type === 'text').map(r => r.text).join('\n');
        if (!transcribed.trim()) {
          showNotification('No speech detected', 'error');
          micBtn.classList.remove('transcribing');
          micBtn.setAttribute('aria-pressed', 'false');
          convRecording = null;
          return;
        }

        // Translate
        const srcLang = speaker === 'a' ? convLangA : convLangB;
        const tgtLang = speaker === 'a' ? convLangB : convLangA;
        const tgtLangName = (availableLanguages.find(l => l.code === tgtLang) || {}).name || tgtLang;

        const trReq = await fetch('/api/translate', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            text: transcribed,
            source_lang: (availableLanguages.find(l => l.code === srcLang) || {}).name || srcLang,
            target_lang: tgtLangName,
            model: modelSel.value || undefined,
            ...apiCredentials(),
          }),
        });
        if (!trReq.ok) {
          showNotification('Translation failed', 'error');
          micBtn.classList.remove('transcribing');
          micBtn.setAttribute('aria-pressed', 'false');
          convRecording = null;
          return;
        }
        const trReqData = await trReq.json();
        const translation = trReqData.translated_text;

        convHistory.push({ speaker, source: transcribed, translation });
        renderConvTranscript();

        // Auto-TTS
        if (convAutoTts) playConvTts(translation, tgtLang);

      } finally {
        micBtn.classList.remove('transcribing');
        micBtn.setAttribute('aria-pressed', 'false');
        convRecording = null;
      }
    });

    recorder.start();
  } catch (e) {
    if (stream) stream.getTracks().forEach(t => t.stop());
    convRecording = null;
    convMediaRecorder = null;
    micBtn.classList.remove('recording');
    micBtn.setAttribute('aria-pressed', 'false');
    const msg = e.name === 'NotAllowedError' ? 'Microphone access denied'
              : e.name === 'NotFoundError'   ? 'No microphone found'
              : 'Could not start recording';
    showNotification(msg, 'error');
  }
}

convMicA.addEventListener('click', () => {
  if (convMicA.classList.contains('recording')) {
    if (convMediaRecorder && convMediaRecorder.state === 'recording') convMediaRecorder.stop();
    return;
  }
  if (convMicA.classList.contains('transcribing')) return;
  startConvRecording('a');
});

convMicB.addEventListener('click', () => {
  if (convMicB.classList.contains('recording')) {
    if (convMediaRecorder && convMediaRecorder.state === 'recording') convMediaRecorder.stop();
    return;
  }
  if (convMicB.classList.contains('transcribing')) return;
  startConvRecording('b');
});

function renderConvTranscript() {
  convTranscriptA.innerHTML = '';
  convTranscriptB.innerHTML = '';
  for (const entry of convHistory) {
    const bubble = document.createElement('div');
    bubble.className = 'conv-bubble';
    const srcP = document.createElement('p');
    srcP.className = 'conv-source';
    srcP.textContent = entry.source;
    const tgtP = document.createElement('p');
    tgtP.className = 'conv-translation';
    tgtP.textContent = '\u2192 ' + entry.translation;
    bubble.appendChild(srcP);
    bubble.appendChild(tgtP);
    if (entry.speaker === 'a') {
      convTranscriptA.appendChild(bubble);
    } else {
      convTranscriptB.appendChild(bubble);
    }
  }
  convTranscriptA.scrollTop = convTranscriptA.scrollHeight;
  convTranscriptB.scrollTop = convTranscriptB.scrollHeight;
}

async function playConvTts(text, langCode) {
  if (!text.trim()) return;
  const hasTts = (sessionActive && !!userTtsEndpoint)
    || (serverTtsConfigured && serverTtsLanguages.includes(langCode));
  if (!hasTts) return;

  // Stop any existing conv TTS
  if (convTtsAudio) {
    convTtsAudio.pause();
    convTtsAudio.src = '';
    convTtsAudio = null;
  }
  if (convTtsObjUrl) {
    URL.revokeObjectURL(convTtsObjUrl);
    convTtsObjUrl = null;
  }

  try {
    const body = { text, target_lang: langCode };
    if (userTtsEndpoint && userTtsApiKey) {
      body.tts_endpoint = userTtsEndpoint;
      body.tts_api_key  = userTtsApiKey;
    }
    const res = await fetch('/api/tts', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!res.ok) return;
    const blob = await res.blob();
    convTtsObjUrl = URL.createObjectURL(blob);
    convTtsAudio = new Audio(convTtsObjUrl);
    convTtsAudio.addEventListener('ended', () => {
      convTtsAudio = null;
      URL.revokeObjectURL(convTtsObjUrl);
      convTtsObjUrl = null;
    });
    convTtsAudio.play().catch(() => {});
  } catch (_) {}
}

convClearBtn.addEventListener('click', () => {
  convHistory = [];
  renderConvTranscript();
});

convExportBtn.addEventListener('click', () => {
  if (convHistory.length === 0) { showNotification('No conversation to export', 'error'); return; }
  const lines = convHistory.map(e => {
    const label = e.speaker === 'a'
      ? (convLangAsel.options[convLangAsel.selectedIndex]?.text || 'Speaker A')
      : (convLangBsel.options[convLangBsel.selectedIndex]?.text || 'Speaker B');
    return `${label}: ${e.source}\n\u2192 ${e.translation}`;
  });
  const blob = new Blob([lines.join('\n\n')], { type: 'text/plain' });
  const url  = URL.createObjectURL(blob);
  const a    = document.createElement('a');
  a.href     = url;
  a.download = 'conversation.txt';
  a.click();
  URL.revokeObjectURL(url);
});

// ── Mode Tabs ─────────────────────────────────────────────────────────────
function switchTab(tab) {
  activeTab = tab;
  const isText = tab === 'text';
  const isFile = tab === 'file';
  const isConv = tab === 'conversation';

  tabTextBtn.classList.toggle('active', isText);
  tabDocumentBtn.classList.toggle('active', isFile);
  tabConvBtn.classList.toggle('active', isConv);

  // Source panel content areas
  sourceText.closest('.textarea-container').classList.toggle('hidden', !isText);
  docUploadArea.classList.toggle('hidden', !isFile);
  convPanelEl.classList.toggle('hidden', !isConv);

  // Source header / footer elements
  sourceLangInfo.classList.toggle('hidden', !isText);
  voiceBtn.classList.toggle('hidden', !isText);
  uploadLabel.classList.toggle('hidden', !isText);
  srcTtsBtn.classList.add('hidden');
  translateBtn.classList.toggle('hidden', !isText);
  charCount.classList.toggle('hidden', !isText);
  contextHintToggle.classList.toggle('hidden', !isText);
  // Hide context hint row when leaving text mode
  if (!isText) contextHintRow.classList.add('hidden');
  // Hide entire source footer in conversation mode (conv-panel has its own)
  sourcePanelFooter.classList.toggle('hidden', isConv);

  // Source panel full-width stretch in conversation mode
  sourcePanel.classList.toggle('conv-mode', isConv);

  // Swap button: hidden in conv, inert in file
  if (isConv) {
    swapBtn.classList.add('hidden');
    swapBtn.classList.remove('inert');
  } else if (isFile) {
    swapBtn.classList.remove('hidden');
    swapBtn.classList.add('inert');
  } else {
    swapBtn.classList.remove('hidden', 'inert');
  }

  // Output panel
  outputPanel.classList.toggle('hidden', isConv);
  outputPanel.classList.toggle('doc-mode', isFile);
  outputDiv.classList.toggle('hidden', isFile);
  // Reset output toolbar
  copyBtn.classList.add('hidden');
  mdToggleBtn.classList.add('hidden');
  paraToggleBtn.classList.add('hidden');
  incompleteBadge.classList.add('hidden');
  chunkProgress.classList.add('hidden');
  stopTts();

  // Reset para/md view when leaving text mode
  if (!isText) {
    paraViewActive = false;
    paraToggleBtn.setAttribute('aria-pressed', 'false');
    paraOutput.classList.add('hidden');
    outputDiv.classList.remove('hidden');
  }

  if (isFile) {
    ttsBtn.classList.add('hidden');
    resultsDocList.classList.remove('hidden');
    if (!resultsDocList.querySelector('.doc-file-list')) {
      resultsDocList.innerHTML = '<div class="doc-list-placeholder">Translated files will appear here\u2026</div>';
    }
  } else if (isConv) {
    ttsBtn.classList.add('hidden');
    resultsDocList.classList.add('hidden');
  } else {
    if (!resultsDocList.querySelector('.doc-file-list')) {
      resultsDocList.classList.add('hidden');
      resultsDocList.innerHTML = '';
    }
    updateTtsButtonVisibility();
    // Restore MD render if it was active
    if (mdRenderActive && lastOutputText) applyMdRender();
  }
}

tabTextBtn.addEventListener('click', () => switchTab('text'));
tabDocumentBtn.addEventListener('click', () => switchTab('file'));
tabConvBtn.addEventListener('click', () => switchTab('conversation'));


// ── Language Picker ───────────────────────────────────────────────────────
function setTargetLang(code, triggerTranslate = false) {
  selectedTargetLangCode = code;
  const lang = availableLanguages.find(l => l.code === code);
  langPickerLabel.textContent = lang ? lang.name : code;
  // refresh checkmarks if dropdown is open
  if (langPickerOpen) renderLangList(langSearchInput.value);
  if (triggerTranslate) {
    lastTranslatedText = '';
    lastOutputText = '';
    translate(true);
  }
  updateTtsButtonVisibility();
}

function renderLangList(filter) {
  const q = filter.trim().toLowerCase();
  const filtered = availableLanguages
    .filter(l => l.code !== 'auto')
    .filter(l => !q || l.name.toLowerCase().includes(q) || l.code.toLowerCase().includes(q));

  langListEl.innerHTML = '';
  if (filtered.length === 0) {
    const li = document.createElement('li');
    li.className = 'lang-no-results';
    li.textContent = 'No languages found';
    langListEl.appendChild(li);
    return;
  }
  for (const lang of filtered) {
    const li = document.createElement('li');
    li.className = 'lang-list-item' + (lang.code === selectedTargetLangCode ? ' selected' : '');
    li.dataset.code = lang.code;
    li.setAttribute('role', 'option');
    li.setAttribute('aria-selected', lang.code === selectedTargetLangCode ? 'true' : 'false');
    li.tabIndex = -1;
    const nameSpan = document.createElement('span');
    nameSpan.textContent = lang.name;
    li.appendChild(nameSpan);
    if (serverTtsLanguages.includes(lang.code)) {
      const icon = document.createElement('span');
      icon.className = 'lang-tts-icon';
      icon.title = 'TTS available';
      icon.textContent = '\uD83D\uDD0A';
      li.appendChild(icon);
    }
    li.addEventListener('click', () => {
      setTargetLang(lang.code, true);
      closeLangPicker();
      langPickerBtn.focus();
    });
    li.addEventListener('keydown', e => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        setTargetLang(lang.code, true);
        closeLangPicker();
        langPickerBtn.focus();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        closeLangPicker();
        langPickerBtn.focus();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        const next = li.nextElementSibling;
        if (next) next.focus();
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        const prev = li.previousElementSibling;
        if (prev) prev.focus(); else langSearchInput.focus();
      }
    });
    langListEl.appendChild(li);
  }
}

function openLangPicker() {
  langPickerDropdown.classList.remove('hidden');
  langPickerBtn.setAttribute('aria-expanded', 'true');
  langSearchInput.value = '';
  renderLangList('');
  langPickerOpen = true;
  langSearchInput.focus();
  requestAnimationFrame(() => {
    const selected = langListEl.querySelector('.selected');
    if (selected) selected.scrollIntoView({ block: 'nearest' });
  });
}

function closeLangPicker() {
  langPickerDropdown.classList.add('hidden');
  langPickerBtn.setAttribute('aria-expanded', 'false');
  langPickerOpen = false;
}

langPickerBtn.addEventListener('click', () => {
  if (langPickerOpen) closeLangPicker(); else openLangPicker();
});

langSearchInput.addEventListener('input', () => renderLangList(langSearchInput.value));

langSearchInput.addEventListener('keydown', e => {
  if (e.key === 'Escape') {
    e.preventDefault();
    closeLangPicker();
    langPickerBtn.focus();
  } else if (e.key === 'ArrowDown') {
    e.preventDefault();
    const first = langListEl.querySelector('.lang-list-item');
    if (first) first.focus();
  }
});

document.addEventListener('click', e => {
  if (langPickerOpen && !document.getElementById('lang-picker').contains(e.target)) {
    closeLangPicker();
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

// ── Title cycling ────────────────────────────────────────────────────────────
(function () {
  const words = [
    'Translation',   // English
    'Übersetzung',   // German
    'Traduction',    // French
    'Traducción',    // Spanish
    'Tradução',      // Portuguese
    'Traduzione',    // Italian
    'Перевод',       // Russian
    'Μετάφραση',     // Greek
    'Çeviri',        // Turkish
    'Překlad',       // Czech
    '翻訳',           // Japanese
    '번역',           // Korean
    '翻译',           // Chinese (Simplified)
    'ترجمه',         // Persian
    'ترجمة',         // Arabic
    'תרגום',         // Hebrew
    'Tafsiri',       // Swahili
    'अनुवाद',        // Hindi
    'மொழிபெயர்ப்பு', // Tamil
    'Fordítás',      // Hungarian
    'Käännös',       // Finnish
    'Översättning',  // Swedish
    'Vertaling',     // Dutch
  ];
  const el = document.querySelector('.app-title');
  if (!el) return;
  let idx = 0;
  setInterval(() => {
    el.style.opacity = '0';
    setTimeout(() => {
      idx = (idx + 1) % words.length;
      el.textContent = words[idx];
      el.style.opacity = '1';
    }, 350);
  }, 2000);
}());
