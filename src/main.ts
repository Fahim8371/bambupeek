import './styles.css';
import { Channel, invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LogicalSize } from '@tauri-apps/api/dpi';

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const video = $<HTMLVideoElement>('camera');
const dialog = $<HTMLDialogElement>('setup-dialog');
interface Config { ip: string; accessCode: string; serial: string }
interface Printer { ip: string; serial: string; model: string }
interface PrintStatus { state: string | null; progress: number | null; layer: number | null; totalLayers: number | null; remainingMinutes: number | null; nozzle: number | null; bed: number | null }
type StreamEvent = { type: 'format'; codec: string } | { type: 'error'; code: string } | { type: 'status'; values: PrintStatus } | { type: 'status-unavailable'; reason: string };

const errors: Record<string, string> = {
  'store-unavailable': 'The local printer settings could not be opened. You can still connect for this session.',
  'save-failed': 'The printer could not be saved on this device. Your current connection is still available; try saving again in settings.',
  'forget-failed': 'The saved printer could not be removed. Check access to the local settings folder and try again.',
  'saved-printer-invalid': 'The saved printer could not be read. Forget it in settings, then save the connection again.',
  'session-ended': 'Connect the printer before saving it.',
  'invalid-address': 'Enter a valid printer IP address, such as 192.168.1.100.',
  'local-address-required': 'Use the printer’s private address on your local network.',
  'invalid-code': 'The LAN access code must be 8 letters or numbers.',
  'invalid-serial': 'Check the printer serial number. Use letters and numbers only.',
  'ffmpeg-missing': 'The video engine is missing. Install FFmpeg with “brew install ffmpeg”, then reopen BambuPeek.',
  'ffmpeg-start-failed': 'The video engine could not start. Check your FFmpeg installation.',
  'access-rejected': 'The printer rejected the access code. Check the code on its network settings screen.',
  'network-blocked': 'macOS could not reach the printer. Allow BambuPeek in System Settings → Privacy & Security → Local Network, and check that both devices are on the same network.',
  'camera-disabled': 'The camera port is closed. Turn on LAN Only Liveview on your printer, then try again.',
  'printer-timeout': 'The printer did not send video in time. Check the network and LAN Only Liveview setting, then retry.',
  'camera-failed': 'The camera could not open. Check the access code and LAN Only Liveview setting, then try again.',
  'camera-ended': 'The printer closed the camera connection. You can reconnect below.',
  'unsupported-video': 'The printer did not send a supported H.264 video stream.',
  'playback-stalled': 'Playback stopped responding. Try reconnecting to start a fresh stream.',
  'video-format': 'This system cannot play the camera’s video format.',
  'video-decode': 'Video arrived, but this system could not decode it. Try reconnecting.',
  'discovery-unavailable': 'Discovery is unavailable. Check Local Network access, or enter the printer details manually.',
};
const describe = (error: unknown) => errors[String(error)] ?? 'Something interrupted the connection. Please try again.';
let generation = 0;
let disposePlayer: (() => void) | undefined;
let hasConnection = false;
let connecting = false;
let live = false;
let statsReceived = false;
let statusConfigured = false;
let statusFailure: string | undefined;
let toastTimer = 0;
let saved: Pick<Config, 'ip' | 'serial'> | null = null;
let storeBusy = false;
let unreadableSaved = false;
function updateSavedUI() {
  $('saved-profile').hidden = !saved && !unreadableSaved;
  $('saved-address').textContent = saved?.ip ?? 'Saved profile unavailable';
  $<HTMLButtonElement>('connect-saved').disabled = storeBusy || !saved;
  $<HTMLButtonElement>('forget-printer').disabled = storeBusy;
  $('save-current').hidden = !live;
  $<HTMLButtonElement>('save-current').disabled = storeBusy;
  $<HTMLButtonElement>('submit').disabled = storeBusy;
}
async function saveCurrent(sessionId = generation) {
  if (storeBusy) return;
  storeBusy = true; updateSavedUI();
  try {
    saved = await invoke<Pick<Config, 'ip' | 'serial'>>('save_printer', { sessionId });
    unreadableSaved = false; toast('Printer saved on this device. It will reconnect next time.');
  } catch (error) { toast(describe(error)); }
  finally { storeBusy = false; updateSavedUI(); }
}
$('save-current').addEventListener('click', () => void saveCurrent());
$('forget-printer').addEventListener('click', async () => {
  if (storeBusy) return;
  storeBusy = true; updateSavedUI();
  try {
    await invoke('forget_printer'); saved = null; unreadableSaved = false;
    $<HTMLInputElement>('remember-printer').checked = false;
    toast('Saved printer removed. The current session can stay connected.');
  } catch (error) { toast(describe(error)); }
  finally { storeBusy = false; updateSavedUI(); }
});
$('connect-saved').addEventListener('click', async () => {
  if (storeBusy) return;
  storeBusy = true; updateSavedUI();
  try {
    saved = await invoke<Pick<Config, 'ip' | 'serial'> | null>('saved_printer');
    if (saved) { statusConfigured = Boolean(saved.serial); dialog.close(); await start(null); }
  } catch (error) { toast(describe(error)); }
  finally { storeBusy = false; updateSavedUI(); }
});

function toast(text: string) { $('toast').textContent = text; $('toast').hidden = false; clearTimeout(toastTimer); toastTimer = window.setTimeout(() => $('toast').hidden = true, 3500); }
function cameraState(text: string, isLive = false) { document.body.classList.toggle('camera-live', isLive); $('camera-state').textContent = text; $('camera-badge').classList.toggle('live', isLive); }
function resetStats() {
  statsReceived = false; statusFailure = undefined;
  document.body.classList.remove('has-status');
  $('print-state').textContent = 'Ready when you are'; $('print-detail').textContent = 'Connect your camera to get started';
  $('progress-number').textContent = '—'; $('remaining').textContent = '— remaining';
  $('layer').textContent = '— / —'; $('nozzle').textContent = '— °C'; $('bed').textContent = '— °C'; $('status-note').textContent = 'Awaiting connection'; $('status-symbol').textContent = '○';
}
function statusUnavailable(reason?: string) {
  statusFailure = reason ?? statusFailure;
  const hints: Record<string, string> = {
    'serial-needed': 'Add the printer serial number in settings',
    'no-reports': 'No status received — check the printer serial number',
    'access-rejected': 'Status access rejected — check the LAN access code',
    'subscription-rejected': 'Printer denied status access',
    'tls-failed': 'Could not connect securely to printer status',
    'network-blocked': 'Printer status port is unreachable',
    'status-timeout': 'Status connection timed out — retrying',
    'connection-lost': 'Status connection interrupted — retrying',
  };
  if (statsReceived) {
    $('status-note').textContent = 'Last received values · Reconnecting';
  } else {
    $('print-state').textContent = statusFailure ? 'Print status unavailable' : 'Connecting print status…';
    $('status-note').textContent = hints[statusFailure ?? ''] ?? (statusConfigured ? 'Waiting for printer data' : 'Finding printer status on your network');
  }
}
function renderStatus(s: PrintStatus) {
  statsReceived = true; statusFailure = undefined;
  document.body.classList.add('has-status');
  $('print-state').textContent = s.state ?? 'Printer connected';
  const printing = s.state === 'Printing' || s.state === 'Paused' || s.state === 'Preparing';
  $('print-detail').textContent = printing ? 'Keeping an eye on your print' : s.state === 'Complete' ? 'Your print is ready' : 'Nothing printing right now';
  $('progress-number').textContent = s.progress === null ? '—' : `${s.progress}%`;
  const minutes = s.remainingMinutes;
  $('remaining').textContent = minutes === null ? '— remaining' : !printing ? (s.state === 'Complete' ? 'Print complete' : 'No active print') : `${Math.floor(minutes / 60) ? `${Math.floor(minutes / 60)}h ` : ''}${minutes % 60}m remaining`;
  $('layer').textContent = `${s.layer ?? '—'} / ${s.totalLayers ?? '—'}`;
  $('nozzle').textContent = `${s.nozzle === null ? '—' : Math.round(s.nozzle)} °C`;
  $('bed').textContent = `${s.bed === null ? '—' : Math.round(s.bed)} °C`;
  $('status-note').textContent = ''; $('status-symbol').textContent = printing ? '◔' : s.state === 'Complete' ? '✓' : '○';
}
async function stop() {
  generation++; connecting = false; live = false; updateSavedUI();
  disposePlayer?.(); disposePlayer = undefined;
  await invoke('disconnect').catch(() => undefined);
  $('loading').hidden = true; $('resolution').hidden = true; cameraState('Offline');
}
function failed(code: string, token: number) {
  if (token !== generation) return;
  void stop();
  $('empty').hidden = false; $('empty-title').textContent = 'Let’s get you connected.'; $('empty-copy').textContent = describe(code);
  $('open-setup').hidden = hasConnection; $('retry').hidden = !hasConnection;
  $('print-state').textContent = 'Camera disconnected'; $('print-detail').textContent = 'Open settings to check your connection'; $('status-note').textContent = 'Disconnected';
}

async function start(config: Config | null, remember = false) {
  if (connecting) return;
  await stop(); resetStats();
  connecting = true;
  if (config) statusConfigured = Boolean(config.serial);
  const token = ++generation;
  $('empty').hidden = true; $('loading').hidden = false; cameraState('Connecting');
  const media = new MediaSource();
  const url = URL.createObjectURL(media);
  const abort = new AbortController();
  const signal = abort.signal;
  video.src = url;
  let source: SourceBuffer | undefined;
  let codec: string | undefined;
  let queue: ArrayBuffer[] = [];
  let pendingBytes = 0;
  let appending = false;
  let presented = false;
  let frameHandle = 0;
  let frameCount = 0;
  let lastFrame = performance.now();
  let diagnosticTimer = 0;
  let starting = true;
  const startupTimer = window.setTimeout(() => failed('printer-timeout', token), 25000);
  const ack = () => { void invoke('acknowledge', { sessionId: token }).catch(() => undefined); };
  const flush = () => {
    if (!source || source.updating || token !== generation) return;
    if (source.buffered.length && video.currentTime - source.buffered.start(0) > 12) {
      source.remove(source.buffered.start(0), video.currentTime - 6); return;
    }
    const next = queue.shift();
    if (!next) return;
    pendingBytes -= next.byteLength;
    try { appending = true; source.appendBuffer(next); } catch { failed('video-decode', token); }
  };
  const initialize = () => {
    if (!codec || media.readyState !== 'open' || source) return;
    const mime = `video/mp4; codecs="${codec}"`;
    if (!MediaSource.isTypeSupported(mime)) { failed('video-format', token); return; }
    try {
      source = media.addSourceBuffer(mime);
      source.addEventListener('error', () => failed('video-decode', token), { signal });
      source.addEventListener('updateend', () => {
        if (appending) { appending = false; ack(); }
        if (source && source.buffered.length) {
          const end = source.buffered.end(source.buffered.length - 1);
          const start = source.buffered.start(0);
          if (starting && end - start > .2) { starting = false; video.currentTime = Math.max(start, end - .4); void video.play().catch(() => failed('video-decode', token)); }
          else if (end - video.currentTime > 2.5) { video.currentTime = Math.max(start, end - .5); }
        }
        flush();
      }, { signal });
      flush();
    } catch { failed('video-format', token); }
  };
  media.addEventListener('sourceopen', initialize, { signal });
  video.addEventListener('error', () => failed('video-decode', token), { signal });
  const onFrame = () => {
    if (token !== generation) return;
    frameCount++; lastFrame = performance.now();
    if (!presented) {
      presented = true; live = true; connecting = false; clearTimeout(startupTimer);
      $('loading').hidden = true; cameraState('Live', true); $('resolution').hidden = false;
      if (!statsReceived) statusUnavailable();
      updateSavedUI();
      if (remember) void saveCurrent(token);
    }
    frameHandle = video.requestVideoFrameCallback(onFrame);
  };
  frameHandle = video.requestVideoFrameCallback(onFrame);
  diagnosticTimer = window.setInterval(() => {
    if (token !== generation || !presented) return;
    const lag = video.buffered.length ? Math.max(0, video.buffered.end(video.buffered.length - 1) - video.currentTime) : 0;
    $('resolution').textContent = `${video.videoWidth} × ${video.videoHeight} · ${lag.toFixed(1)}s buffer`;
    if (performance.now() - lastFrame > 12000) failed('playback-stalled', token);
    void invoke('playback_sample', { width: video.videoWidth, height: video.videoHeight, frames: frameCount }).catch(() => undefined);
  }, 3000);
  disposePlayer = () => {
    abort.abort(); clearTimeout(startupTimer); clearInterval(diagnosticTimer); video.cancelVideoFrameCallback(frameHandle);
    video.pause(); video.removeAttribute('src'); video.load(); URL.revokeObjectURL(url); queue = [];
  };
  const events = new Channel<StreamEvent>();
  events.onmessage = event => {
    if (token !== generation) return;
    if (event.type === 'format') { codec = event.codec; initialize(); }
    else if (event.type === 'error') failed(event.code, token);
    else if (event.type === 'status') renderStatus(event.values);
    else statusUnavailable(event.reason);
  };
  const data = new Channel<ArrayBuffer>();
  data.onmessage = bytes => {
    if (token !== generation) return;
    pendingBytes += bytes.byteLength;
    if (pendingBytes > 8 * 1024 * 1024) { failed('playback-stalled', token); return; }
    queue.push(bytes); flush();
  };
  try {
    await invoke('connect', { config, sessionId: token, events, video: data });
    hasConnection = true;
    $<HTMLInputElement>('access-code').value = '';
  } catch (error) { failed(String(error), token); }
}

$('setup-form').addEventListener('submit', event => {
  event.preventDefault();
  const config: Config = { ip: $<HTMLInputElement>('ip').value.trim(), accessCode: $<HTMLInputElement>('access-code').value.trim(), serial: $<HTMLInputElement>('serial').value.trim() };
  $('form-error').hidden = true;
  if (!/^\d{1,3}(\.\d{1,3}){3}$/.test(config.ip) || !/^[a-z0-9]{8}$/i.test(config.accessCode)) {
    $('form-error').textContent = 'Enter the printer IP address and its 8-character access code.'; $('form-error').hidden = false; return;
  }
  dialog.close(); void start(config, $<HTMLInputElement>('remember-printer').checked);
});
const openSetup = () => { $('form-error').hidden = true; updateSavedUI(); dialog.showModal(); };
$('settings').addEventListener('click', openSetup); $('open-setup').addEventListener('click', openSetup);
$('close-setup').addEventListener('click', () => dialog.close());
$('retry').addEventListener('click', () => void start(null));
$('cancel-connect').addEventListener('click', async () => { await stop(); $('empty').hidden = false; $('retry').hidden = !hasConnection; $('open-setup').hidden = hasConnection; resetStats(); });
const sizes = { small: [480, 270], medium: [800, 450], large: [1120, 630] } as const;
type SizePreset = keyof typeof sizes;
function closeSizes() { $('size-options').hidden = true; $('size-toggle').setAttribute('aria-expanded', 'false'); }
async function setSize(preset: SizePreset) {
  const [width, height] = sizes[preset];
  try {
    await getCurrentWindow().setSize(new LogicalSize(width, height));
    localStorage.setItem('bambupeek.size', preset);
    document.querySelectorAll<HTMLButtonElement>('[data-size]').forEach(button => button.setAttribute('aria-pressed', String(button.dataset.size === preset)));
    $('size-toggle').title = `Window size: ${preset}`;
    closeSizes(); showControls();
  } catch { toast('Could not resize the window.'); }
}
$('size-toggle').addEventListener('click', () => {
  const opening = $('size-options').hidden;
  $('size-options').hidden = !opening; $('size-toggle').setAttribute('aria-expanded', String(opening));
});
document.querySelectorAll<HTMLButtonElement>('[data-size]').forEach(button => button.addEventListener('click', () => void setSize(button.dataset.size as SizePreset)));
document.addEventListener('pointerdown', event => { if (!(event.target as Element).closest('.size-control')) closeSizes(); });
window.addEventListener('keydown', event => { if (event.key === 'Escape' && !$('size-options').hidden) { closeSizes(); $('size-toggle').focus(); event.stopImmediatePropagation(); } });
let pinned = false;
let pinChanging = false;
let controlsTimer = 0;
function showControls() {
  document.body.classList.add('controls-visible');
  clearTimeout(controlsTimer);
  controlsTimer = window.setTimeout(() => document.body.classList.remove('controls-visible'), 1800);
}
async function setPinned(next: boolean) {
  if (pinChanging || next === pinned) return;
  pinChanging = true;
  $<HTMLButtonElement>('pin').disabled = true;
  try {
    await getCurrentWindow().setAlwaysOnTop(next);
    pinned = next;
    $('pin').setAttribute('aria-pressed', String(next));
    $('pin').title = next ? 'Turn off Stay on top (Esc)' : 'Stay on top';
    localStorage.setItem('bambupeek.pin', String(next));
    showControls();
  } catch { toast('Could not change the window setting.'); }
  finally { pinChanging = false; $<HTMLButtonElement>('pin').disabled = false; }
}
$('pin').addEventListener('click', () => void setPinned(!pinned));
$('stage').addEventListener('pointermove', showControls);
$('stage').addEventListener('pointerleave', () => { clearTimeout(controlsTimer); document.body.classList.remove('controls-visible'); });
const dragWindow = (event: PointerEvent) => {
  if (event.button === 0) void getCurrentWindow().startDragging().catch(() => undefined);
};
$('move-window').addEventListener('pointerdown', dragWindow);
video.addEventListener('pointerdown', dragWindow);
$('minimize-window').addEventListener('click', () => void getCurrentWindow().minimize().catch(() => toast('Could not minimize the window.')));
$('close-window').addEventListener('click', () => void getCurrentWindow().close().catch(() => toast('Could not close the window.')));
window.addEventListener('keydown', event => { if (event.key === 'Escape' && !dialog.open && pinned) void setPinned(false); });

// Place the overlay on the picture itself, including when the video is letterboxed.
function positionOverlay() {
  const stage = $('stage');
  const width = stage.clientWidth, height = stage.clientHeight;
  const ratio = video.videoWidth && video.videoHeight ? video.videoWidth / video.videoHeight : width / height;
  const pictureWidth = Math.min(width, height * ratio);
  const pictureHeight = pictureWidth / ratio;
  stage.style.setProperty('--picture-left', `${(width - pictureWidth) / 2}px`);
  stage.style.setProperty('--picture-bottom', `${(height - pictureHeight) / 2}px`);
  stage.style.setProperty('--picture-width', `${pictureWidth}px`);
}
new ResizeObserver(positionOverlay).observe($('stage'));
video.addEventListener('loadedmetadata', positionOverlay);

$('discover').addEventListener('click', async () => {
  const button = $<HTMLButtonElement>('discover'); button.disabled = true; button.textContent = 'Looking for printers…';
  const result = $('discovery-results'); result.replaceChildren();
  try {
    const printers = await invoke<Printer[]>('discover');
    if (!printers.length) { const p = document.createElement('p'); p.className = 'discovery-note'; p.textContent = 'No printers found yet. You can enter the details below.'; result.append(p); }
    for (const printer of printers) {
      const b = document.createElement('button'); b.type = 'button'; b.className = 'discovery-item';
      const name = document.createElement('span'); name.textContent = printer.model;
      const detail = document.createElement('small'); detail.textContent = `${printer.ip}  →`; b.append(name, detail);
      b.addEventListener('click', () => { $<HTMLInputElement>('ip').value = printer.ip; $<HTMLInputElement>('serial').value = printer.serial; $<HTMLInputElement>('access-code').focus(); toast('Printer selected. Enter its LAN access code.'); });
      result.append(b);
    }
  } catch (error) { const p = document.createElement('p'); p.className = 'discovery-note'; p.textContent = describe(error); result.append(p); }
  finally { button.disabled = false; button.textContent = '⌁  Find printers on this network'; }
});
window.addEventListener('beforeunload', () => { disposePlayer?.(); void invoke('disconnect'); });
async function init() {
  const size = localStorage.getItem('bambupeek.size');
  if (size && Object.hasOwn(sizes, size)) await setSize(size as SizePreset);
  if (localStorage.getItem('bambupeek.pin') === 'true') {
    await setPinned(true);
  }
  try {
    const env = await invoke<{ ffmpeg: boolean }>('environment');
    if (!env.ffmpeg) { $('empty-copy').textContent = describe('ffmpeg-missing'); }
    try {
      saved = await invoke<Pick<Config, 'ip' | 'serial'> | null>('saved_printer');
      if (saved) {
        $<HTMLInputElement>('ip').value = saved.ip; $<HTMLInputElement>('serial').value = saved.serial;
        statusConfigured = Boolean(saved.serial); hasConnection = true;
        if (env.ffmpeg) await start(null);
      }
    } catch (error) { unreadableSaved = true; toast(describe(error)); }
    updateSavedUI();
  } catch { $('empty-copy').textContent = 'Open BambuPeek as a desktop app to connect your printer.'; }
}
void init();
