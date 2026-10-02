// Shared by every window: Tauri access, the Windows 98 title bar, menus,
// small pixel icons and the synthesized sounds.

const T = window.__TAURI__;
const invoke = T.core.invoke;
const listen = T.event.listen;
const thisWindow = T.window.getCurrentWindow();

// ---------- themes: Classic (aim.css as written), Dark, and the user's own colors ----------

const THEMES = {
  classic: {},
  dark: {
    face: '#2b2b2b', hilite: '#4a4a4a', light: '#363636', shadow: '#141414', dark: '#000000',
    text: '#e8e8e8', muted: '#a8a8a8', pane: '#1c1c1c', 'select-text': '#ffffff', notice: '#3a3620',
    select: '#2f5fb3', 'title-a': '#1a2850', 'title-b': '#2d5a98', 'title-off-a': '#333333', 'title-off-b': '#4d4d4d',
    me: '#ff7474', them: '#7eaaff', link: '#8cb8ff',
  },
};
/// The colors people can change (Setup > Colors), in order.
const COLOR_NAMES = [
  ['face', 'Windows'], ['text', 'Text'], ['muted', 'Quieter text'], ['pane', 'Text boxes and lists'],
  ['title-a', 'Title bar'], ['title-b', 'Title bar fade'], ['select', 'Selection'],
  ['me', 'Your name in IMs'], ['them', 'Their name in IMs'], ['link', 'Links'], ['notice', 'Notices'],
];
const THEME_VARS = [...new Set([...Object.keys(THEMES.dark), ...COLOR_NAMES.map(c => c[0])])];

/// Sets the theme on this window; remembered locally, so new windows open in it at once.
function applyTheme(prefs) {
  const all = { ...(THEMES[prefs?.theme] || {}), ...(prefs?.colors || {}) };
  const root = document.documentElement.style;
  for (const k of THEME_VARS) all[k] ? root.setProperty('--' + k, all[k]) : root.removeProperty('--' + k);
  document.documentElement.dataset.theme = prefs?.theme || 'classic';
  try { localStorage.setItem('him-theme', JSON.stringify({ theme: prefs?.theme, colors: prefs?.colors })); } catch {}
}
try { applyTheme(JSON.parse(localStorage.getItem('him-theme') || 'null')); } catch {}
invoke('get_settings').then(s => applyTheme(s.prefs)).catch(() => {});
listen('prefs', e => applyTheme(e.payload));

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function param(name) {
  return new URLSearchParams(location.search).get(name) ?? '';
}

// ---------- icons (16px grid, drawn for this app) ----------

const ICON = {
  h: `<svg viewBox="-3 -1 16 16"><path d="M0 2L3 0V13H0Z M7 2H10V12L7 14Z M3 6H7V9H3Z" fill="#8a0000" stroke="#3a0000" stroke-width="0.5" stroke-linejoin="round"/><path d="M0 2L2 0.667V13H0Z M7 2H9V12.667L7 14Z M0 6H9V8H0Z" fill="#f01408"/></svg>`,
  min: `<svg viewBox="0 0 8 7"><rect x="0" y="5" width="6" height="2"/></svg>`,
  close: `<svg viewBox="0 0 8 7"><path d="M0 0h2v1h1v1h2v-1h1v-1h2v1h-1v1h-1v1h-1v1h1v1h1v1h1v1h-2v-1h-1v-1h-2v1h-1v1h-2v-1h1v-1h1v-1h1v-1h-1v-1h-1v-1h-1z"/></svg>`,
  im: `<svg viewBox="0 0 22 22"><path d="M2 3h18v11H9l-4 4v-4H2z" fill="#fff8c0" stroke="#000"/><path d="M5 6h12M5 9h9M5 12h6" stroke="#000080"/></svg>`,
  chat: `<svg viewBox="0 0 22 22"><path d="M1 2h13v8H6l-3 3v-3H1z" fill="#c8e0ff" stroke="#000"/><path d="M8 8h13v8h-2v3l-3-3H8z" fill="#fff8c0" stroke="#000"/></svg>`,
  info: `<svg viewBox="0 0 22 22"><circle cx="11" cy="11" r="9" fill="#fff" stroke="#000080" stroke-width="1.5"/><rect x="10" y="9" width="2.5" height="8" fill="#000080"/><rect x="10" y="5" width="2.5" height="2.5" fill="#000080"/></svg>`,
  away: `<svg viewBox="0 0 22 22"><path d="M4 2h11l3 3v15H4z" fill="#fff8a0" stroke="#000"/><path d="M15 2v3h3" fill="none" stroke="#000"/><path d="M7 8h8M7 11h8M7 14h5" stroke="#806000"/></svg>`,
  back: `<svg viewBox="0 0 22 22"><circle cx="11" cy="11" r="8" fill="#ffe000" stroke="#000"/><circle cx="8" cy="9" r="1.2"/><circle cx="14" cy="9" r="1.2"/><path d="M7 13q4 4 8 0" fill="none" stroke="#000" stroke-width="1.3"/></svg>`,
  addbuddy: `<svg viewBox="0 0 22 22"><circle cx="9" cy="7" r="4" fill="#ffd9a0" stroke="#000"/><path d="M2 20q0-7 7-7t7 7z" fill="#4060c0" stroke="#000"/><path d="M17 6v8M13 10h8" stroke="#008000" stroke-width="2.5"/></svg>`,
  block: `<svg viewBox="0 0 22 22"><circle cx="11" cy="11" r="8" fill="#fff" stroke="#c00000" stroke-width="2.5"/><path d="M5.5 5.5l11 11" stroke="#c00000" stroke-width="2.5"/></svg>`,
  file: `<svg viewBox="0 0 22 22"><path d="M5 2h8l4 4v14H5z" fill="#fff" stroke="#000"/><path d="M13 2v4h4" fill="#c0c0c0" stroke="#000"/><path d="M8 11h6M8 14h6M8 17h4" stroke="#000080"/></svg>`,
  send: `<svg viewBox="0 0 22 22"><path d="M2 10L20 3L14 20L11 13Z" fill="#fff" stroke="#000"/><path d="M11 13L20 3" stroke="#000"/></svg>`,
  doorOpen: `<svg viewBox="0 0 16 16" width="14" height="14"><rect x="2" y="1" width="11" height="14" fill="#5a3a18"/><path d="M3 2h4l3 2v11l-7-1z" fill="#c8883a" stroke="#3a2008" stroke-width=".8"/><circle cx="8" cy="9" r=".8" fill="#ffd000"/></svg>`,
  doorShut: `<svg viewBox="0 0 16 16" width="14" height="14"><rect x="3" y="1" width="10" height="14" fill="#c8883a" stroke="#3a2008" stroke-width=".8"/><rect x="5" y="3" width="6" height="4" fill="none" stroke="#7a4a18" stroke-width=".7"/><rect x="5" y="9" width="6" height="4" fill="none" stroke="#7a4a18" stroke-width=".7"/><circle cx="11" cy="8.5" r=".8" fill="#ffd000"/></svg>`,
  note: `<svg viewBox="0 0 16 16" width="13" height="13"><path d="M2 1h9l3 3v11H2z" fill="#fff8a0" stroke="#000" stroke-width=".8"/><path d="M4 6h7M4 8.5h7M4 11h5" stroke="#806000" stroke-width=".8"/></svg>`,
  busy: `<svg viewBox="0 0 16 16" width="13" height="13"><circle cx="8" cy="8" r="6" fill="#d00000" stroke="#600" stroke-width=".8"/><rect x="4" y="7" width="8" height="2" fill="#fff"/></svg>`,
  pending: `<svg viewBox="0 0 16 16" width="13" height="13"><circle cx="8" cy="8" r="6" fill="#fff" stroke="#000" stroke-width=".8"/><path d="M8 4v4l3 2" fill="none" stroke="#000" stroke-width="1.2"/></svg>`,
  lock: `<svg viewBox="0 0 16 16" width="12" height="12"><path d="M4.5 7V5a3.5 3.5 0 0 1 7 0v2" fill="none" stroke="#404040" stroke-width="1.6"/><rect x="3" y="7" width="10" height="8" fill="#e0b000" stroke="#604000" stroke-width=".8"/></svg>`,
  unlock: `<svg viewBox="0 0 16 16" width="12" height="12"><path d="M4.5 7V5a3.5 3.5 0 0 1 7 0" fill="none" stroke="#404040" stroke-width="1.6"/><rect x="3" y="7" width="10" height="8" fill="#c0c0c0" stroke="#404040" stroke-width=".8"/></svg>`,
  pencil: `<svg viewBox="0 0 16 16" width="12" height="12"><path d="M2 14l1-4 8-8 3 3-8 8z" fill="#ffd000" stroke="#000" stroke-width=".8"/><path d="M2 14l1-4 3 3z" fill="#ffd9a0" stroke="#000" stroke-width=".8"/></svg>`,
  plug: `<svg viewBox="0 0 16 16" width="12" height="12"><path d="M5 1v4M11 1v4" stroke="#404040" stroke-width="1.6"/><path d="M3 5h10v3a5 5 0 0 1-10 0z" fill="#808080" stroke="#202020" stroke-width=".8"/><path d="M8 12v3" stroke="#202020" stroke-width="1.6"/></svg>`,
  triOpen: `<svg viewBox="0 0 9 9" width="9" height="9"><path d="M1 2h7l-3.5 4z"/></svg>`,
  triShut: `<svg viewBox="0 0 9 9" width="9" height="9"><path d="M2 1v7l4-3.5z"/></svg>`,
  warn: `<svg viewBox="0 0 32 32"><path d="M16 2L31 29H1Z" fill="#ffe000" stroke="#000"/><rect x="14.5" y="10" width="3" height="11"/><rect x="14.5" y="23" width="3" height="3"/></svg>`,
  question: `<svg viewBox="0 0 32 32"><circle cx="16" cy="16" r="14" fill="#fff" stroke="#000080" stroke-width="2"/><text x="16" y="23" font-size="20" font-weight="bold" text-anchor="middle" fill="#000080" font-family="Times New Roman, serif">?</text></svg>`,
};

// ---------- window chrome ----------

function setupWindow({ title, minimize = true, onClose } = {}) {
  const bar = $('.titlebar');
  bar.setAttribute('data-tauri-drag-region', '');
  bar.innerHTML = `<span class="ticon">${ICON.h}</span><span class="ttext" data-tauri-drag-region>${esc(title)}</span>` +
    (minimize ? `<button class="tbtn min" title="Minimize">${ICON.min}</button>` : '') +
    `<button class="tbtn close" title="Close">${ICON.close}</button>`;
  if (minimize) $('.tbtn.min', bar).onclick = () => thisWindow.minimize();
  $('.tbtn.close', bar).onclick = () => (onClose ? onClose() : thisWindow.close());
  setTitle(title);
  window.addEventListener('focus', () => document.body.classList.remove('inactive'));
  window.addEventListener('blur', () => document.body.classList.add('inactive'));
  if (!document.hasFocus()) document.body.classList.add('inactive');
  document.addEventListener('keydown', e => {
    if (e.key === 'Escape' && document.body.dataset.escCloses !== 'no') (onClose ? onClose() : thisWindow.close());
  });
  document.addEventListener('contextmenu', e => {
    if (!e.target.closest('input, textarea, .selectable')) e.preventDefault();
  });
}

function setTitle(title) {
  const t = $('.titlebar .ttext');
  if (t) t.textContent = title;
  document.title = title;
  thisWindow.setTitle(title).catch(() => {});
}

// Menus: <div class="menubar"><div class="menu" data-menu="file">...
function setupMenus(defs) {
  const bar = $('.menubar');
  bar.innerHTML = defs.map((m, i) =>
    `<div class="menu" data-i="${i}"><div class="mtitle">${esc(m.title)}</div><div class="mlist">${
      m.items.map((it, j) => it === '-' ? '<div class="msep"></div>' :
        `<div class="mitem${it.disabled ? ' disabled' : ''}${it.checked ? ' checked' : ''}" data-j="${j}">${esc(it.label)}</div>`).join('')
    }</div></div>`).join('');
  let open = null;
  const close = () => { if (open) open.classList.remove('open'); open = null; };
  $$('.menu', bar).forEach(menu => {
    const m = defs[menu.dataset.i];
    $('.mtitle', menu).onmousedown = e => {
      e.stopPropagation();
      if (open === menu) return close();
      close();
      menu.classList.add('open');
      open = menu;
      // refresh dynamic state (checked/disabled) right before showing
      if (m.refresh) m.refresh(m.items);
      $$('.mitem', menu).forEach(el => {
        const it = m.items[el.dataset.j];
        el.classList.toggle('disabled', !!(typeof it.disabled === 'function' ? it.disabled() : it.disabled));
        el.classList.toggle('checked', !!(typeof it.checked === 'function' ? it.checked() : it.checked));
      });
    };
    $('.mtitle', menu).onmouseenter = () => {
      if (open && open !== menu) { close(); menu.classList.add('open'); open = menu; }
    };
    // A press inside the open menu must not reach the document's close handler, or a
    // plain click (press, release on an item) would close the menu before the release.
    $('.mlist', menu).onmousedown = e => e.stopPropagation();
    $$('.mitem', menu).forEach(el => {
      el.onmouseup = e => {
        e.stopPropagation();
        if (el.classList.contains('disabled')) return;
        close();
        m.items[el.dataset.j].action?.();
      };
    });
  });
  document.addEventListener('mousedown', close);
  window.addEventListener('blur', close);
}

// ---------- message boxes inside a window ----------

function alertBox(text, { icon = 'warn', title = 'HIM' } = {}) {
  return new Promise(resolve => {
    const host = document.createElement('div');
    host.style.cssText = 'position:fixed;inset:0;display:grid;place-items:center;background:rgba(0,0,0,0.001);z-index:100';
    host.innerHTML = `<div style="position:relative;width:min(92vw,280px);background:var(--face);padding:3px;box-shadow:inset -1px -1px var(--dark),inset 1px 1px var(--light),inset -2px -2px var(--shadow),inset 2px 2px var(--hilite)">
      <div class="titlebar" style="background:linear-gradient(90deg,var(--title-a),var(--title-b));color: var(--select-text)"><span class="ttext">${esc(title)}</span></div>
      <div style="display:flex;gap:10px;padding:10px 8px 6px;align-items:flex-start"><span class="dlg-icon">${ICON[icon] || ''}</span><div class="selectable" style="flex:1;user-select:text;-webkit-user-select:text;white-space:pre-wrap;word-break:break-word">${esc(text)}</div></div>
      <div style="display:flex;justify-content:center;padding:4px 0 6px"><button class="default">OK</button></div></div>`;
    document.body.appendChild(host);
    const ok = $('button', host);
    ok.focus();
    const done = () => { host.remove(); resolve(); };
    ok.onclick = done;
    host.addEventListener('keydown', e => { if (e.key === 'Enter' || e.key === 'Escape') { e.stopPropagation(); done(); } });
  });
}

function confirmBox(text, { yes = 'Yes', no = 'No', icon = 'question', title = 'HIM' } = {}) {
  return new Promise(resolve => {
    const host = document.createElement('div');
    host.style.cssText = 'position:fixed;inset:0;display:grid;place-items:center;z-index:100';
    host.innerHTML = `<div style="width:min(92vw,280px);background:var(--face);padding:3px;box-shadow:inset -1px -1px var(--dark),inset 1px 1px var(--light),inset -2px -2px var(--shadow),inset 2px 2px var(--hilite)">
      <div class="titlebar" style="background:linear-gradient(90deg,var(--title-a),var(--title-b));color: var(--select-text)"><span class="ttext">${esc(title)}</span></div>
      <div style="display:flex;gap:10px;padding:10px 8px 6px;align-items:flex-start"><span class="dlg-icon">${ICON[icon] || ''}</span><div style="flex:1;white-space:pre-wrap">${esc(text)}</div></div>
      <div style="display:flex;justify-content:center;gap:6px;padding:4px 0 6px"><button class="default y">${esc(yes)}</button><button class="n">${esc(no)}</button></div></div>`;
    document.body.appendChild(host);
    const finish = v => { host.remove(); resolve(v); };
    $('.y', host).onclick = () => finish(true);
    $('.n', host).onclick = () => finish(false);
    $('.y', host).focus();
    host.addEventListener('keydown', e => { if (e.key === 'Escape') { e.stopPropagation(); finish(false); } });
  });
}

// ---------- sounds: the classic recordings (ui/sounds), the user's own, or synthesized ----------

const Sound = (() => {
  let ctx;
  const ac = () => (ctx ??= new AudioContext());

  function tone(freq, start, dur, { type = 'sine', gain = 0.2, to, curve = 'exp' } = {}) {
    const c = ac(), o = c.createOscillator(), g = c.createGain();
    o.type = type;
    o.frequency.setValueAtTime(freq, c.currentTime + start);
    if (to) o.frequency[curve === 'exp' ? 'exponentialRampToValueAtTime' : 'linearRampToValueAtTime'](to, c.currentTime + start + dur);
    g.gain.setValueAtTime(0.0001, c.currentTime + start);
    g.gain.exponentialRampToValueAtTime(gain, c.currentTime + start + 0.01);
    g.gain.exponentialRampToValueAtTime(0.0001, c.currentTime + start + dur);
    o.connect(g).connect(c.destination);
    o.start(c.currentTime + start);
    o.stop(c.currentTime + start + dur + 0.02);
  }

  function creak(start, dur, from, to, gain = 0.12) {
    const c = ac(), o = c.createOscillator(), f = c.createBiquadFilter(), g = c.createGain(), lfo = c.createOscillator(), lg = c.createGain();
    o.type = 'sawtooth';
    o.frequency.setValueAtTime(from, c.currentTime + start);
    o.frequency.linearRampToValueAtTime(to, c.currentTime + start + dur);
    lfo.frequency.value = 38;
    lg.gain.value = from * 0.18;
    lfo.connect(lg).connect(o.frequency);
    f.type = 'bandpass';
    f.frequency.value = 900;
    f.Q.value = 4;
    g.gain.setValueAtTime(0.0001, c.currentTime + start);
    g.gain.exponentialRampToValueAtTime(gain, c.currentTime + start + 0.04);
    g.gain.exponentialRampToValueAtTime(0.0001, c.currentTime + start + dur);
    o.connect(f).connect(g).connect(c.destination);
    [o, lfo].forEach(n => { n.start(c.currentTime + start); n.stop(c.currentTime + start + dur + 0.05); });
  }

  function thud(start, gain = 0.5) {
    tone(140, start, 0.18, { gain, to: 55 });
    tone(320, start, 0.05, { type: 'triangle', gain: gain * 0.4, to: 120 });
  }

  // The user's own files (Setup > Sounds), decoded once; they replace the built-in sound.
  const custom = new Map();
  async function loadCustom(prefs) {
    custom.clear();
    for (const kind of Object.keys(prefs.customSounds || {})) {
      try {
        const bytes = await invoke('sound_data', { kind });
        custom.set(kind, await ac().decodeAudioData(bytes instanceof ArrayBuffer ? bytes : new Uint8Array(bytes).buffer));
      } catch {}
    }
  }
  function playCustom(kind) {
    const buf = custom.get(kind);
    if (!buf) return false;
    const src = ac().createBufferSource();
    src.buffer = buf;
    src.connect(ac().destination);
    src.start();
    return true;
  }

  // The recordings that come with HIM; a room line you send sounds like an IM you send.
  const builtIn = new Map();
  const builtInFiles = { buddyIn: 'sounds/buddy-in.wav', buddyOut: 'sounds/buddy-out.wav', imSend: 'sounds/im.mp3', imReceive: 'sounds/im-receive.mp3', chatSend: 'sounds/im.mp3' };
  async function loadBuiltIn() {
    for (const [kind, file] of Object.entries(builtInFiles)) {
      try {
        const r = await fetch(file);
        builtIn.set(kind, await ac().decodeAudioData(await r.arrayBuffer()));
      } catch {}
    }
  }
  loadBuiltIn();
  /// The user's file first, then the recording; false if neither (the synthesized one plays).
  function playFile(kind) {
    if (playCustom(kind)) return true;
    const buf = builtIn.get(kind);
    if (!buf) return false;
    const src = ac().createBufferSource();
    src.buffer = buf;
    src.connect(ac().destination);
    src.start();
    return true;
  }

  let enabled = true;
  return {
    set enabled(v) { enabled = v; },
    get enabled() { return enabled; },
    loadCustom,
    doorOpen() { if (!enabled || playFile('buddyIn')) return; creak(0, 0.42, 190, 430); thud(0.36, 0.25); },
    doorClose() { if (!enabled || playFile('buddyOut')) return; creak(0, 0.22, 380, 210, 0.09); thud(0.2, 0.55); },
    imReceive() { if (!enabled || playFile('imReceive')) return; tone(1318, 0, 0.09, { gain: 0.16, to: 1100 }); tone(880, 0.1, 0.16, { gain: 0.16, to: 700 }); },
    imSend() { if (!enabled || playFile('imSend')) return; tone(660, 0, 0.06, { type: 'triangle', gain: 0.12, to: 990 }); },
    chatSend() { if (!enabled || playFile('chatSend')) return; tone(660, 0, 0.06, { type: 'triangle', gain: 0.12, to: 990 }); },
    alert() { if (!enabled) return; tone(988, 0, 0.12, { type: 'square', gain: 0.05 }); tone(784, 0.13, 0.18, { type: 'square', gain: 0.05 }); },
    /// Setup's "Play" buttons: the sound for an event, ignoring the on/off switch.
    preview(kind) {
      const was = enabled; enabled = true;
      ({ buddyIn: this.doorOpen, buddyOut: this.doorClose, imReceive: this.imReceive, imSend: this.imSend })[kind]?.call(this);
      enabled = was;
    },
  };
})();

async function loadSoundPref() {
  try {
    const p = (await invoke('get_settings')).prefs;
    Sound.enabled = p.sounds;
    Sound.loadCustom(p);
  } catch {}
  listen('prefs', e => { Sound.enabled = e.payload.sounds; Sound.loadCustom(e.payload); });
}

function fmtTime(ts) {
  return new Date(ts * 1000).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
}

function utf8Len(s) { return new TextEncoder().encode(s).length; }

// Links (a.url, in chat lines and notices) open in the browser; HIM's windows never navigate.
document.addEventListener('click', e => {
  const a = e.target.closest && e.target.closest('a.url');
  if (!a) return;
  e.preventDefault();
  invoke('open_link', { url: a.dataset.url }).catch(() => {});
});

// ---------- "a newer HIM is out" (the startup update check, src-tauri/src/updates.rs) ----------

/// Puts a one-line notice into `el` when a newer release exists, with a link to it.
function updateNotice(el) {
  if (!el) return;
  const show = u => {
    if (!u) return;
    el.innerHTML = `HIM ${esc(u.version)} is out. <a class="url" href="#" data-url="${esc(u.url)}">Download</a>`;
    el.hidden = false;
  };
  invoke('update_available').then(show).catch(() => {});
  listen('update', e => show(e.payload));
}
