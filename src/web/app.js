// Winsploit web view. The terminal owns the session: this page renders the
// state it streams (/events) and sends actions back (/api/action). The tool
// selection, category, filter, shown terminal, grid, scroll and find are
// local to this tab.
'use strict';

const $ = (id) => document.getElementById(id);

const S = {
  tools: [],
  tabs: [],
  rank: [],
  slots: 4,
  st: null,
  sel: -1,
  tab: 'all',
  filter: '',
  focus: 0, // terminal shown: 0 = console, 1..4
  grid: false,
  terms: new Map(), // sid -> { el, lines, follow }
  panes: new Map(), // pos -> pane element
  unread: new Set(), // positions with output not seen yet
  seenSids: new Set(),
  expect: null, // { tool }: a run this tab asked for; show its terminal when it appears
  argsFor: null,
  find: { text: '', hits: [], hit: -1 },
  role: null, // 'owner' (this machine), or a LAN guest's 'control' / 'watch'
  name: '',
  admin: null, // owner only: { key, guests, url, fingerprint }
  dismissed: new Set(), // join requests put off here (they stay in Guests)
  announced: new Set(), // join requests already notified
  collapsed: new Set(), // tool-list categories folded (by category id)
  catsOpen: false, // category chips unfolded past two rows
};

const store = {
  get(k) { try { return localStorage.getItem(k); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* private mode */ } },
};

// the terminal's category symbols (ui.rs cat_icon): monochrome, theme-coloured
const ICONS = {
  all: '≡', installed: '✓', osint: '◎', recon: '⌖', network: '⇄', web: '⊕',
  exploit: '↯', passwords: '⊛', reversing: '⟲', pwn: '▲', mobile: '▯',
  forensics: '⌕', crypto: '⌗', stego: '◈', wordlists: '▤',
};
const WORD = { installed: '✓ ready', missing: '+ get', conflict: '! PATH', unknown: '…' };
const LAUNCH = {
  stream: ['Command line', 'Run'],
  gui: ['Desktop app', 'Launch'],
  web: ['Opens in your browser', 'Open'],
  dataset: ['Wordlists', 'Show files'],
};

// ---------------------------------------------------------------- theme

// Firefox keeps a running color transition on its old value when a variable
// on :root changes, so colors switch with transitions off for a frame.
function instantly() {
  const root = document.documentElement;
  root.classList.add('retheme');
  void root.offsetWidth;
  setTimeout(() => root.classList.remove('retheme'), 60);
}

let theme = store.get('ws-theme') || 'auto';
function applyTheme() {
  const root = document.documentElement;
  instantly();
  if (theme === 'auto') delete root.dataset.theme; else root.dataset.theme = theme;
  const b = $('theme-toggle');
  b.dataset.mode = theme;
  b.title = `Theme: ${theme}`;
}
applyTheme();
function setTheme(v) {
  theme = v;
  store.set('ws-theme', theme);
  applyTheme();
  if ($('settings-dialog').open) renderSettingsForm();
}
$('theme-toggle').addEventListener('click', () => setTheme({ auto: 'light', light: 'dark', dark: 'auto' }[theme] || 'auto'));
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
  if ($('settings-dialog').open) renderSettingsForm();
});

// -------------------------------------------------------- notifications

let notify = store.get('ws-notify') === 'on' && 'Notification' in window && Notification.permission === 'granted';
function renderNotify() {
  const b = $('notify-toggle');
  b.classList.toggle('on', notify);
  b.title = `Desktop notifications: ${notify ? 'on (when this tab is in the background)' : 'off'}`;
}
renderNotify();
async function setNotify(on) {
  if (!('Notification' in window)) {
    toast('This browser has no desktop notifications.', 'err');
    on = false;
  } else if (on) {
    const p = Notification.permission === 'default' ? await Notification.requestPermission() : Notification.permission;
    on = p === 'granted';
    if (!on) toast('Notifications are blocked for this page in the browser settings.', 'err');
  }
  notify = on;
  store.set('ws-notify', notify ? 'on' : 'off');
  renderNotify();
  if ($('settings-dialog').open) $('set-notify').checked = notify;
}
$('notify-toggle').addEventListener('click', () => setNotify(!notify));

const loadJSON = (k, fallback) => {
  try { return JSON.parse(store.get(k)) ?? fallback; } catch { return fallback; }
};

// ------------------------------------------------------------- settings
// Page-local (this browser only), like the theme. Nothing goes to winsploit.

// Palettes: a few numbers each (see the top of app.css). The ids match the
// design options P1..P10.
const PALETTES = [
  { id: 'classic', name: 'Terracotta', h: 25, s: '10%', ah: 15, as: '62%', al: '62%', ll: '50%' },
  { id: 'ice', name: 'Ice', h: 212, s: '24%', ah: 189, as: '85%', al: '55%', ll: '36%' },
  { id: 'phosphor', name: 'Phosphor', h: 150, s: '14%', ah: 146, as: '62%', al: '55%', ll: '33%' },
  { id: 'violet', name: 'Ultraviolet', h: 258, s: '22%', ah: 262, as: '88%', al: '72%', ll: '52%' },
  { id: 'indigo', name: 'Indigo night', h: 232, s: '24%', ah: 222, as: '90%', al: '70%', ll: '50%', lift: '2.5%' },
  { id: 'redteam', name: 'Red team', h: 350, s: '8%', ah: 352, as: '82%', al: '62%', ll: '46%' },
  { id: 'amber', name: 'Amber CRT', h: 38, s: '16%', ah: 38, as: '95%', al: '56%', ll: '36%' },
  { id: 'deepsea', name: 'Deep sea', h: 192, s: '30%', ah: 170, as: '70%', al: '50%', ll: '30%' },
  { id: 'graphite', name: 'Graphite', h: 0, s: '0%', ah: 82, as: '75%', al: '55%', ll: '32%' },
  { id: 'frost', name: 'Frost', h: 220, s: '17%', ah: 193, as: '45%', al: '68%', ll: '38%', lift: '13%' },
];

// one hue per category for its title in the tool list (same in every palette)
const CAT_HUE = {
  osint: 195, recon: 165, network: 135, web: 85, exploit: 0, passwords: 30, reversing: 275,
  pwn: 325, mobile: 225, forensics: 50, crypto: 300, stego: 250, wordlists: 210,
};
function catHue(id) {
  if (id in CAT_HUE) return CAT_HUE[id];
  let h = 0;
  for (const ch of id) h = (h * 31 + ch.charCodeAt(0)) % 360;
  return h;
}

const DEFAULTS = {
  palette: 'classic', catColors: true, density: 'comfortable', uiSize: 14,
  desc: true, catsMode: 'wrap',
  termSize: 13, wrap: true, follow: true, scrollback: 5000, confirmClose: true,
  suggest: true, explain: true,
  sidebar: true,
};
let SET = { ...DEFAULTS, ...loadJSON('ws-settings', {}) };
const saveSettings = () => store.set('ws-settings', JSON.stringify(SET));

function applySettings() {
  const root = document.documentElement;
  instantly();
  const p = PALETTES.find((x) => x.id === SET.palette) || PALETTES[0];
  for (const k of ['h', 's', 'ah', 'as', 'al', 'll']) root.style.setProperty(`--${k}`, String(p[k]));
  root.style.setProperty('--lift', p.lift || '0%');
  root.dataset.palette = p.id;
  root.dataset.density = SET.density;
  root.dataset.catcolors = SET.catColors ? 'on' : 'off';
  root.style.setProperty('--ui-size', `${SET.uiSize}px`);
  root.style.setProperty('--term-size', `${SET.termSize}px`);
  const body = document.body;
  body.dataset.desc = SET.desc ? 'on' : 'off';
  body.dataset.wrap = SET.wrap ? 'on' : 'off';
  body.dataset.sidebar = SET.sidebar ? 'on' : 'off';
  const sb = $('toggle-sidebar');
  sb.setAttribute('aria-expanded', String(SET.sidebar));
  sb.title = `${SET.sidebar ? 'Hide' : 'Show'} the tool list (b)`;
  const d = $('toggle-desc');
  d.setAttribute('aria-pressed', String(SET.desc));
  d.title = SET.desc ? 'Hide descriptions: one line per tool' : 'Show descriptions';
}
applySettings();

function setSetting(key, value) {
  SET[key] = value;
  saveSettings();
  applySettings();
  if (key === 'catsMode' || key === 'sidebar' || key === 'uiSize' || key === 'density') layoutCats();
  if (key === 'scrollback') for (const t of S.terms.values()) trimTerm(t);
  if (key === 'explain') renderExplain();
  if (key === 'suggest' && !value) closeSuggest();
  if ($('settings-dialog').open) renderSettingsForm();
}

// --------------------------------------------------------- panel sizes

let LAY = loadJSON('ws-layout', {});
const saveLayout = () => store.set('ws-layout', JSON.stringify(LAY));
const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));

function applyLayout() {
  const root = document.documentElement;
  if (LAY.side) root.style.setProperty('--side-w', `${LAY.side}px`); else root.style.removeProperty('--side-w');
  const card = $('tool-card');
  card.classList.toggle('sized', !!LAY.card);
  if (LAY.card) card.style.setProperty('--card-h', `${LAY.card}px`);
  const flags = $('flags');
  flags.classList.toggle('sized', !!LAY.flags);
  if (LAY.flags) flags.style.setProperty('--flags-h', `${LAY.flags}px`);
  const wrap = document.querySelector('.term-wrap');
  wrap.style.setProperty('--gx', `${LAY.gx || 50}%`);
  wrap.style.setProperty('--gy', `${LAY.gy || 50}%`);
}
applyLayout();

/**
 * A drag handle. `start()` reads the size when the drag begins; `move(start,
 * delta)` sets the new one from the pointer (or arrow-key) offset in px.
 */
function splitter(el, axis, start, move, reset) {
  const done = () => { saveLayout(); layoutCats(); };
  el.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    try { el.setPointerCapture(e.pointerId); } catch { /* synthetic pointer */ }
    el.classList.add('drag');
    document.body.classList.add('resizing', axis === 'x' ? 'rx' : 'ry');
    const s = start();
    const x0 = e.clientX;
    const y0 = e.clientY;
    const onMove = (ev) => move(s, axis === 'x' ? ev.clientX - x0 : ev.clientY - y0);
    const onUp = () => {
      el.removeEventListener('pointermove', onMove);
      el.classList.remove('drag');
      document.body.classList.remove('resizing', 'rx', 'ry');
      done();
    };
    el.addEventListener('pointermove', onMove);
    el.addEventListener('pointerup', onUp, { once: true });
    el.addEventListener('pointercancel', onUp, { once: true });
  });
  el.addEventListener('dblclick', () => { reset(); applyLayout(); done(); });
  el.addEventListener('keydown', (e) => {
    const k = { ArrowLeft: -1, ArrowUp: -1, ArrowRight: 1, ArrowDown: 1 }[e.key];
    if (!k) return;
    e.preventDefault();
    e.stopPropagation();
    move(start(), k * (e.shiftKey ? 64 : 16));
    done();
  });
}

splitter($('split-side'), 'x',
  () => $('sidebar').getBoundingClientRect().width,
  (s, d) => { LAY.side = Math.round(clamp(s + d, 220, Math.min(640, window.innerWidth * 0.6))); applyLayout(); },
  () => { delete LAY.side; });
splitter($('split-card'), 'y',
  () => $('tool-card').getBoundingClientRect().height,
  (s, d) => {
    const room = document.querySelector('.main').clientHeight - 32 - 14 - 160;
    LAY.card = Math.round(clamp(s + d, 96, Math.max(96, room)));
    applyLayout();
  },
  () => { delete LAY.card; });
splitter($('split-flags'), 'y',
  () => $('flags').getBoundingClientRect().height,
  (s, d) => { LAY.flags = Math.round(clamp(s - d, 60, $('sidebar').clientHeight * 0.7)); applyLayout(); },
  () => { delete LAY.flags; });
splitter($('split-gx'), 'x',
  () => LAY.gx || 50,
  (s, d) => { LAY.gx = clamp(s + (d / document.querySelector('.term-wrap').clientWidth) * 100, 15, 85); applyLayout(); },
  () => { delete LAY.gx; });
splitter($('split-gy'), 'y',
  () => LAY.gy || 50,
  (s, d) => { LAY.gy = clamp(s + (d / document.querySelector('.term-wrap').clientHeight) * 100, 15, 85); applyLayout(); },
  () => { delete LAY.gy; });

$('toggle-sidebar').addEventListener('click', () => setSetting('sidebar', !SET.sidebar));
$('toggle-desc').addEventListener('click', () => setSetting('desc', !SET.desc));

// ------------------------------------------------------- settings dialog

function effectiveDark() {
  return theme === 'dark' || (theme === 'auto' && window.matchMedia('(prefers-color-scheme: dark)').matches);
}

function renderSettingsForm() {
  const form = $('settings-form');
  for (const el of form.querySelectorAll('[data-set]')) {
    const v = SET[el.dataset.set];
    if (el.type === 'checkbox') el.checked = !!v;
    else if (el.type === 'radio') el.checked = String(v) === el.value;
    else el.value = String(v);
  }
  for (const r of form.querySelectorAll('input[name="theme"]')) r.checked = r.value === theme;
  $('set-notify').checked = notify;
  const box = $('palettes');
  const dark = effectiveDark();
  box.replaceChildren(...PALETTES.map((p) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'swatch';
    b.setAttribute('role', 'radio');
    b.setAttribute('aria-checked', String(p.id === SET.palette));
    b.title = p.name;
    const sw = document.createElement('span');
    sw.className = 'sw';
    const bg = document.createElement('i');
    const lift = parseFloat(p.lift || '0');
    bg.style.setProperty('background', dark ? `hsl(${p.h} ${p.s} ${9 + lift}%)` : `hsl(${p.h} ${p.s} 96%)`);
    const ac = document.createElement('i');
    ac.style.setProperty('background', `hsl(${p.ah} ${p.as} ${dark ? p.al : p.ll})`);
    sw.append(bg, ac);
    b.append(sw, p.name);
    b.addEventListener('click', () => setSetting('palette', p.id));
    return b;
  }));
}

$('open-settings').addEventListener('click', openSettings);
function openSettings() {
  renderSettingsForm();
  $('settings-dialog').showModal();
}
$('settings-form').addEventListener('submit', (e) => e.preventDefault());
$('settings-form').addEventListener('change', (e) => {
  const el = e.target;
  if (el.name === 'theme') {
    setTheme(el.value);
    return;
  }
  if (el.id === 'set-notify') {
    setNotify(el.checked);
    return;
  }
  const key = el.dataset.set;
  if (!key) return;
  let v;
  if (el.type === 'checkbox') v = el.checked;
  else if ('num' in el.dataset) v = Number(el.value);
  else v = el.value;
  setSetting(key, v);
});
$('reset-layout').addEventListener('click', () => {
  LAY = {};
  saveLayout();
  applyLayout();
  layoutCats();
  toast('Panel sizes reset');
});
$('reset-settings').addEventListener('click', () => {
  SET = { ...DEFAULTS };
  saveSettings();
  LAY = {};
  saveLayout();
  S.collapsed.clear();
  saveCollapsed();
  S.catsOpen = false;
  store.set('ws-cats-open', '0');
  setTheme('auto');
  applySettings();
  applyLayout();
  renderTools();
  layoutCats();
  renderSettingsForm();
  toast('Settings reset');
});

S.collapsed = new Set(loadJSON('ws-collapsed', []));
S.catsOpen = store.get('ws-cats-open') === '1';
const saveCollapsed = () => store.set('ws-collapsed', JSON.stringify([...S.collapsed]));

// ----------------------------------------------------------- connection

let es = null;
function connect() {
  es = new EventSource('/events');
  es.addEventListener('hello', (e) => onHello(JSON.parse(e.data)));
  es.addEventListener('state', (e) => onState(JSON.parse(e.data)));
  es.addEventListener('out', (e) => onOut(JSON.parse(e.data)));
  es.addEventListener('toast', (e) => onToast(JSON.parse(e.data)));
  es.addEventListener('you', (e) => onYou(JSON.parse(e.data)));
  es.addEventListener('admin', (e) => onAdmin(JSON.parse(e.data)));
  es.addEventListener('end', (e) => {
    es.close();
    setConn('ended');
    let reason = '';
    try { reason = JSON.parse(e.data).reason || ''; } catch { /* plain end */ }
    if (reason === 'removed') {
      ended('The host removed you', 'You no longer have access to this session. Ask the host if you need to get back in.');
    } else if (isGuest()) {
      ended('Session ended', 'The host stopped sharing this session. Ask them for a new join key when it is back.');
    } else {
      ended('Session ended',
        'The terminal ended this session: it quit, or the web view was turned off with w. ' +
        'Press o in winsploit to open a new one.');
    }
  });
  es.onopen = () => setConn('live');
  es.onerror = () => {
    if (es.readyState === EventSource.CLOSED) {
      setConn('ended');
      if (isGuest()) {
        ended('Access ended', 'This browser is no longer let in. Reload the page to join again with a new key.');
      } else {
        ended('Not signed in',
          'This tab has no valid session. Press o in winsploit to open the web view with a fresh link.');
      }
    } else {
      setConn('reconnecting');
    }
  };
}

function setConn(state) {
  const c = $('conn');
  c.dataset.state = state;
  const viewers = S.st ? S.st.viewers : 0;
  $('conn-text').textContent = {
    connecting: 'Connecting…',
    live: viewers > 1 ? `Live · ${viewers} tabs` : 'Live',
    reconnecting: 'Reconnecting…',
    ended: 'Ended',
  }[state];
}

function ended(title, text) {
  $('ended-title').textContent = title;
  $('ended-text').textContent = text;
  $('ended').hidden = false;
}

async function act(action) {
  try {
    const r = await fetch('/api/action', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(action),
    });
    if (!r.ok) toast((await r.text()) || r.statusText, 'err');
  } catch {
    toast('The terminal session is not reachable.', 'err');
  }
}

function toast(text, kind) {
  const t = document.createElement('div');
  t.className = `toast ${kind || ''}`;
  t.textContent = text;
  const box = $('toasts');
  box.append(t);
  while (box.children.length > 3) box.firstChild.remove();
  setTimeout(() => t.remove(), 4000);
}

function onToast(t) {
  toast(t.text, t.kind);
  if (notify && document.hidden && t.kind !== 'info') {
    try { new Notification('winsploit', { body: t.text, icon: '/favicon.svg', tag: 'winsploit' }); } catch { /* ignore */ }
  }
}

// ------------------------------------------------------- who this page is

const isGuest = () => S.role === 'control' || S.role === 'watch';

function onYou(y) {
  S.role = y.role || null;
  S.name = y.name || '';
  document.body.dataset.role = S.role || 'none';
  const b = $('role-badge');
  b.hidden = !isGuest();
  if (S.role === 'watch') {
    b.textContent = 'Watching';
    b.title = 'The host gave you watch-only access: you see everything live, but can\'t run or stop tools.';
  } else if (S.role === 'control') {
    b.textContent = 'Guest · full control';
    b.title = 'The host gave you full control: everything except managing guests.';
  }
  b.className = `role-badge ${S.role || ''}`;
  if (S.role === 'watch') for (const d of document.querySelectorAll('dialog[open]')) d.close();
  renderGuestsButton();
  if (S.st) renderCard();
}

// --------------------------------------------- LAN guests (owner's page)

function renderGuestsButton() {
  const on = S.role === 'owner' && !!(S.st && S.st.lan);
  $('open-guests').hidden = !on;
  const waiting = S.admin ? S.admin.guests.filter((g) => g.status === 'waiting').length : 0;
  const c = $('guests-count');
  c.hidden = !on || waiting === 0;
  c.textContent = waiting;
}

function onAdmin(a) {
  S.admin = a;
  for (const g of a.guests) {
    if (g.status === 'waiting' && !S.announced.has(g.id)) {
      S.announced.add(g.id);
      if (notify && document.hidden) {
        try { new Notification('winsploit', { body: `${guestName(g)} wants to join`, icon: '/favicon.svg', tag: `join-${g.id}` }); } catch { /* ignore */ }
      }
    }
  }
  renderGuestsButton();
  renderRequests();
  if ($('guests-dialog').open) renderGuestsDialog();
}

const guestName = (g) => (g.name ? `“${g.name}”` : g.ip);

/** One guest, as a row with the buttons that make sense for them. */
function guestRow(g) {
  const row = $('guest-tpl').content.firstElementChild.cloneNode(true);
  row.querySelector('.avatar').textContent = (g.name || '?').slice(0, 1).toUpperCase();
  row.querySelector('.g-name').textContent = g.name || 'No name given';
  row.querySelector('.g-meta').textContent = `${g.ip} · ${g.agent}`;
  const state = row.querySelector('.g-state');
  if (g.status === 'waiting') {
    state.textContent = 'waiting';
    state.className = 'g-state waiting';
  } else {
    state.textContent = `${g.role === 'control' ? 'full control' : 'watch only'}${g.online ? '' : ' · offline'}`;
    state.className = `g-state ${g.role}`;
  }
  for (const b of row.querySelectorAll('[data-action="admit"]')) {
    const role = b.dataset.role;
    b.hidden = g.status === 'in' && g.role === role;
    if (g.status === 'in') b.textContent = role === 'control' ? 'Give full control' : 'Make watch only';
    b.classList.toggle('primary', g.status === 'waiting' && role === 'watch');
    b.addEventListener('click', () => act({ type: 'admit', guest: g.id, role }));
  }
  const rm = row.querySelector('[data-action="remove"]');
  rm.textContent = g.status === 'waiting' ? 'Deny' : 'Remove';
  rm.addEventListener('click', () => act({ type: 'remove', guest: g.id }));
  return row;
}

/** Join requests as cards in the corner, until decided or put off. */
function renderRequests() {
  const box = $('requests');
  box.replaceChildren();
  if (!S.admin) return;
  for (const g of S.admin.guests) {
    if (g.status !== 'waiting' || S.dismissed.has(g.id)) continue;
    const card = document.createElement('div');
    card.className = 'request';
    const head = document.createElement('div');
    head.className = 'request-head';
    const title = document.createElement('b');
    title.textContent = `${guestName(g)} wants to join`;
    const later = document.createElement('button');
    later.type = 'button';
    later.className = 'icon-btn sm';
    later.title = 'Decide later (they keep waiting in Guests)';
    later.setAttribute('aria-label', 'Decide later');
    later.innerHTML = '<svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6 6 18"/></svg>';
    later.addEventListener('click', () => { S.dismissed.add(g.id); renderRequests(); });
    head.append(title, later);
    card.append(head, guestRow(g));
    box.append(card);
  }
}

let keyTimer = null;
function renderGuestsDialog() {
  const a = S.admin;
  if (!a) return;
  $('g-url').textContent = a.url || 'no LAN address found';
  $('g-fp').textContent = a.fingerprint ? `${a.fingerprint.slice(0, 47)}…` : '';
  $('g-fp').title = a.fingerprint || '';
  renderKey();
  const list = $('guest-list');
  list.replaceChildren();
  if (!a.guests.length) {
    const e = document.createElement('p');
    e.className = 'muted';
    e.textContent = 'Nobody yet. Give someone the address and the key: their request shows up here and in your terminal.';
    list.append(e);
  }
  for (const g of a.guests) list.append(guestRow(g));
}

function renderKey() {
  const k = S.admin && S.admin.key;
  const code = $('g-key');
  const st = $('g-key-state');
  code.textContent = k ? k.code : 'none';
  let live = !!k && k.state === 'live';
  let text = '';
  if (live) {
    const left = Math.max(0, Math.round((k.expires_ms - Date.now()) / 1000));
    if (left === 0) live = false;
    text = live ? `one use · ${Math.floor(left / 60)}:${String(left % 60).padStart(2, '0')} left` : 'expired';
  } else if (k) {
    text = { used: 'used: make a new one for the next person', expired: 'expired', burned: 'burned after too many wrong tries' }[k.state] || k.state;
  }
  code.classList.toggle('dead', !live);
  st.textContent = text;
  $('copy-invite').disabled = !live || !S.admin.url;
}

$('open-guests').addEventListener('click', () => {
  renderGuestsDialog();
  $('guests-dialog').showModal();
  clearInterval(keyTimer);
  keyTimer = setInterval(() => { if ($('guests-dialog').open) renderKey(); else clearInterval(keyTimer); }, 1000);
});
document.querySelector('[data-action="newkey"]').addEventListener('click', () => act({ type: 'newkey' }));
$('copy-invite').addEventListener('click', async () => {
  const a = S.admin;
  const text = `Join my Winsploit session: open ${a.url} (accept the certificate warning; its SHA-256 starts with ${a.fingerprint.slice(0, 23)}) and enter the key ${a.key.code}. It works once, for 10 minutes.`;
  try {
    await navigator.clipboard.writeText(text);
    toast('Invite copied: address, key and certificate fingerprint');
  } catch {
    toast('Copy failed: the browser blocked clipboard access.', 'err');
  }
});

// --------------------------------------------------------------- events

function onHello(h) {
  S.tools = h.tools;
  S.tabs = h.tabs;
  S.rank = h.rank;
  S.slots = h.slots;
  if (!S.tabs.some((t) => t.key === S.tab)) S.tab = 'all';
  if (S.sel < 0 || !S.tools[S.sel]) {
    const saved = store.get('ws-tool');
    const i = S.tools.findIndex((t) => t.name === saved);
    S.sel = i >= 0 ? i : (visible()[0] ?? 0);
  }
  renderCats();
  renderTools();
  renderCard();
}

function onState(st) {
  const first = S.st === null;
  S.st = st;
  if ($('conn').dataset.state !== 'ended') setConn('live');
  $('lan-badge').hidden = !st.lan;
  renderGuestsButton();
  if (first) {
    // open on a running tool's terminal if there is one
    const running = st.terminals.findIndex((t, i) => i > 0 && t && t.state === 'running');
    S.focus = running > 0 ? running : 0;
  }
  // show the terminal of a run this tab started, once it appears
  if (S.expect) {
    const t = st.terminals.find((t) => t && t.pos > 0 && !S.seenSids.has(t.sid) && t.title === S.expect.tool);
    if (t) {
      S.focus = t.pos;
      S.expect = null;
    }
  }
  for (const t of st.terminals) if (t) S.seenSids.add(t.sid);
  // a terminal that closed can't stay in front
  if (S.focus > 0 && !st.terminals[S.focus] && !S.grid) S.focus = 0;
  renderStats();
  renderCats();
  renderTools();
  renderCard();
  renderTerminals();
  renderFlags();
  renderChallenge();
}

// The active challenge, and switching between existing ones. Creating one
// (which makes folders) stays in the terminal: C.
function renderChallenge() {
  const st = S.st || {};
  const list = st.challenges || [];
  const active = st.challenge;
  const sel = $('challenge');
  const sig = JSON.stringify([list.map((c) => c.id), active && active.id]);
  if (sel.dataset.sig !== sig) {
    sel.dataset.sig = sig;
    sel.replaceChildren();
    if (!active) {
      const o = new Option(list.length ? 'No challenge' : 'No challenges (C in the terminal)', '');
      o.disabled = true;
      o.selected = true;
      sel.append(o);
    }
    for (const c of list) {
      const o = new Option(c.title, c.id);
      o.selected = !!active && active.id === c.id;
      sel.append(o);
    }
    sel.disabled = list.length === 0;
  }
  $('challenge-target').textContent = active && active.target ? active.target : '';
}

$('recipe').addEventListener('change', (e) => {
  const t = S.tools[S.sel];
  const list = (S.st && S.st.recipes && t && S.st.recipes[t.name]) || [];
  const r = list[Number(e.target.value)];
  if (r) {
    $('args').value = r.args;
    $('args').focus();
    closeSuggest();
    renderExplain();
  }
  e.target.value = '';
});

$('challenge').addEventListener('change', (e) => {
  if (e.target.value) act({ type: 'challenge', id: e.target.value });
});

// Flags caught in tool output (newest first). Copying happens here in the
// page; nothing is sent back to winsploit.
function renderFlags() {
  const flags = (S.st && S.st.flags) || [];
  $('flags').hidden = flags.length === 0;
  $('split-flags').hidden = flags.length === 0;
  $('flag-count').textContent = flags.length;
  const list = $('flag-list');
  list.replaceChildren();
  for (const f of flags.slice().reverse()) {
    const li = document.createElement('li');
    li.className = f.known ? 'flag known' : 'flag';
    const code = document.createElement('code');
    code.textContent = f.text;
    const meta = document.createElement('span');
    meta.className = 'flag-meta';
    meta.textContent = `${f.known ? '' : 'likely · '}${f.source} · ${(f.at || '').slice(11, 16)}`;
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'btn small';
    btn.textContent = 'Copy';
    btn.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(f.text);
        btn.textContent = 'Copied';
      } catch {
        // plain-HTTP LAN pages have no clipboard API: select it instead
        const r = document.createRange();
        r.selectNodeContents(code);
        const sel = window.getSelection();
        sel.removeAllRanges();
        sel.addRange(r);
        btn.textContent = 'Selected';
      }
      setTimeout(() => { btn.textContent = 'Copy'; }, 1500);
    });
    li.append(code, meta, btn);
    list.append(li);
  }
}

function onOut(o) {
  const t = term(o.sid);
  if (o.reset) {
    t.lines = [];
    t.el.replaceChildren();
  }
  const frag = document.createDocumentFragment();
  for (const text of o.lines) {
    t.lines.push(text);
    frag.append(lineEl(text));
  }
  t.el.append(frag);
  trimTerm(t);
  const pos = posOf(o.sid);
  if (pos !== null && o.lines.length && !shown(pos)) {
    S.unread.add(pos);
    renderTabs();
  }
  if (pos === S.focus && S.find.text) refreshFind(false);
  if (t.follow && SET.follow) t.el.scrollTop = t.el.scrollHeight;
  else if (o.lines.length) t.el.parentElement?.querySelector('.jump')?.removeAttribute('hidden');
}

/** Keep at most the lines the settings allow (the run log has them all). */
function trimTerm(t) {
  const extra = t.lines.length - SET.scrollback;
  if (extra > 0) {
    t.lines.splice(0, extra);
    for (let i = 0; i < extra; i++) t.el.firstChild?.remove();
  }
}

// ----------------------------------------------------------------- output

function term(sid) {
  let t = S.terms.get(sid);
  if (!t) {
    const el = document.createElement('div');
    el.className = 'out';
    el.tabIndex = 0;
    t = { el, lines: [], follow: true };
    const self = t;
    el.addEventListener('scroll', () => {
      self.follow = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
      if (self.follow) el.parentElement?.querySelector('.jump')?.setAttribute('hidden', '');
    });
    S.terms.set(sid, t);
  }
  return t;
}

function posOf(sid) {
  if (!S.st) return null;
  const i = S.st.terminals.findIndex((t) => t && t.sid === sid);
  return i < 0 ? null : i;
}

function shown(pos) {
  return pos === S.focus || (S.grid && pos > 0);
}

function lineClass(t) {
  if (t.startsWith('[err]')) return 'l-err';
  if (t.startsWith('> ')) return 'l-in';
  if (t.startsWith('$ ')) return 'l-cmd';
  if (t.startsWith('──') || t.startsWith('::')) return 'l-meta';
  return '';
}

const URL_RE = /https?:\/\/[^\s"'<>]+/g;
// The sign-in link is the key to this session: don't show it on a page that
// may be screen-shared. (This tab is already signed in.)
const TOKEN_RE = /([?&]t=)[0-9a-f]{16,}/g;
function lineEl(raw) {
  const text = raw.replace(TOKEN_RE, '$1••••••••');
  const d = document.createElement('div');
  const c = lineClass(text);
  if (c) d.className = c;
  let last = 0;
  // a masked sign-in link isn't a working link: leave it as text
  const links = text === raw ? text.matchAll(URL_RE) : [];
  for (const m of links) {
    d.append(text.slice(last, m.index));
    const a = document.createElement('a');
    a.href = m[0];
    a.textContent = m[0];
    a.target = '_blank';
    a.rel = 'noopener noreferrer';
    d.append(a);
    last = m.index + m[0].length;
  }
  d.append(text.slice(last));
  return d;
}

// --------------------------------------------------------------- find

function shownOut() {
  const t = S.st && S.st.terminals[S.focus];
  return t ? term(t.sid).el : null;
}

function refreshFind(jump) {
  const el = shownOut();
  const needle = S.find.text.trim().toLowerCase();
  S.find.hits = [];
  for (const t of S.terms.values()) {
    for (const row of t.el.querySelectorAll('.hit')) row.classList.remove('hit', 'current');
  }
  if (el && needle) {
    const rows = el.children;
    for (let i = 0; i < rows.length; i++) {
      if (rows[i].textContent.toLowerCase().includes(needle)) {
        rows[i].classList.add('hit');
        S.find.hits.push(i);
      }
    }
  }
  if (!S.find.hits.length) S.find.hit = -1;
  else if (jump || S.find.hit < 0 || S.find.hit >= S.find.hits.length) S.find.hit = S.find.hits.length - 1;
  showHit(jump);
}

function showHit(scroll) {
  const pane = S.panes.get(S.focus);
  const n = S.find.hits.length;
  if (pane) pane.querySelector('.find-count').textContent = S.find.text.trim() ? (n ? `${S.find.hit + 1}/${n}` : 'none') : '';
  const el = shownOut();
  if (!el || S.find.hit < 0) return;
  const row = el.children[S.find.hits[S.find.hit]];
  row.classList.add('current');
  if (scroll) {
    term(S.st.terminals[S.focus].sid).follow = false;
    row.scrollIntoView({ block: 'center' });
  }
}

function stepFind(older) {
  const el = shownOut();
  if (!el || !S.find.hits.length) return;
  el.children[S.find.hits[S.find.hit]]?.classList.remove('current');
  const n = S.find.hits.length;
  S.find.hit = older ? (S.find.hit - 1 + n) % n : (S.find.hit + 1) % n;
  showHit(true);
}

// ------------------------------------------------------------- tool list

function status(i) {
  return S.st && S.st.status[i] ? S.st.status[i] : 'unknown';
}

function catLabel(id) {
  return S.tabs.find((t) => t.key === `cat:${id}`)?.label || id;
}

function inTab(i, key) {
  if (key === 'all') return true;
  if (key === 'installed') return status(i) === 'installed';
  return S.tools[i].categories.includes(key.slice(4));
}

function matches(i) {
  const words = S.filter.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const t = S.tools[i];
  return words.every((w) =>
    t.name.toLowerCase().includes(w) ||
    t.description.toLowerCase().includes(w) ||
    t.categories.some((c) => c.includes(w) || catLabel(c).toLowerCase().includes(w)));
}

// Same order as the terminal: in a category, the tools made for it first;
// elsewhere grouped by main category.
function visibleIn(key) {
  const v = S.tools.map((t, i) => i).filter((i) => inTab(i, key) && matches(i));
  if (key.startsWith('cat:')) {
    const id = key.slice(4);
    v.sort((a, b) => (S.tools[a].primary !== id) - (S.tools[b].primary !== id) || a - b);
  } else {
    v.sort((a, b) => S.rank[a] - S.rank[b] ||
      S.tools[a].primary.localeCompare(S.tools[b].primary) || a - b);
  }
  return v;
}
const visible = () => visibleIn(S.tab);

function renderCats() {
  const nav = $('cats');
  nav.replaceChildren();
  for (const tab of S.tabs) {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'cat';
    b.setAttribute('aria-pressed', String(S.tab === tab.key));
    b.title = tab.blurb;
    const id = tab.key.startsWith('cat:') ? tab.key.slice(4) : tab.key;
    const ic = document.createElement('span');
    ic.className = 'ic';
    ic.textContent = ICONS[id] || '◇';
    if (tab.key.startsWith('cat:')) ic.style.setProperty('--c', String(catHue(id)));
    b.append(ic, tab.label);
    const s = document.createElement('span');
    s.className = 'n';
    s.textContent = visibleIn(tab.key).length;
    b.append(s);
    b.addEventListener('click', () => setTab(tab.key));
    nav.append(b);
  }
  $('cat-blurb').textContent = S.tabs.find((t) => t.key === S.tab)?.blurb || '';
  layoutCats();
}

// Rows: fold to two rows behind "+N more" (opens by itself when the chosen
// category is folded away). One row: sideways with the wheel and arrows.
function layoutCats() {
  const nav = $('cats');
  const more = $('cats-more');
  const wrap = $('cats-wrap');
  const active = nav.querySelector('[aria-pressed="true"]');
  const scroll = SET.catsMode === 'scroll';
  nav.classList.toggle('scroll', scroll);
  if (scroll) {
    nav.classList.remove('folded');
    more.hidden = true;
    if (active) {
      const l = active.offsetLeft - nav.offsetLeft;
      if (l < nav.scrollLeft + 30) nav.scrollLeft = l - 40;
      else if (l + active.offsetWidth > nav.scrollLeft + nav.clientWidth - 30) nav.scrollLeft = l + active.offsetWidth - nav.clientWidth + 40;
    }
    catsArrows();
    return;
  }
  $('cats-prev').hidden = true;
  $('cats-next').hidden = true;
  wrap.classList.remove('fade-l', 'fade-r');
  nav.classList.add('folded');
  if (!nav.clientHeight) return; // not laid out (sidebar hidden)
  const cut = (c) => c.offsetTop - nav.offsetTop + c.offsetHeight > nav.clientHeight;
  const hidden = [...nav.children].filter(cut);
  if (!hidden.length) {
    nav.classList.remove('folded');
    more.hidden = true;
    return;
  }
  const open = S.catsOpen || (active && hidden.includes(active));
  nav.classList.toggle('folded', !open);
  more.hidden = false;
  more.textContent = open ? 'Fewer ▴' : `+${hidden.length} more ▾`;
  more.title = open ? 'Fold the categories to two rows' : 'Show every category';
  more.setAttribute('aria-expanded', String(open));
}

function catsArrows() {
  const nav = $('cats');
  const l = nav.scrollLeft > 2;
  const r = nav.scrollLeft + nav.clientWidth < nav.scrollWidth - 2;
  $('cats-prev').hidden = !l;
  $('cats-next').hidden = !r;
  $('cats-wrap').classList.toggle('fade-l', l);
  $('cats-wrap').classList.toggle('fade-r', r);
}

$('cats-more').addEventListener('click', () => {
  const open = $('cats-more').getAttribute('aria-expanded') === 'true';
  S.catsOpen = !open;
  store.set('ws-cats-open', S.catsOpen ? '1' : '0');
  layoutCats();
});
$('cats').addEventListener('scroll', () => { if (SET.catsMode === 'scroll') catsArrows(); });
$('cats').addEventListener('wheel', (e) => {
  if (SET.catsMode !== 'scroll' || Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
  e.preventDefault();
  $('cats').scrollBy({ left: e.deltaY, behavior: 'instant' });
}, { passive: false });
$('cats-prev').addEventListener('click', () => $('cats').scrollBy({ left: -$('cats').clientWidth * 0.7 }));
$('cats-next').addEventListener('click', () => $('cats').scrollBy({ left: $('cats').clientWidth * 0.7 }));
new ResizeObserver(() => layoutCats()).observe($('sidebar'));

function setTab(key) {
  S.tab = key;
  renderCats();
  const vis = visible();
  if (vis.length && !vis.includes(S.sel)) select(vis[0]);
  renderTools();
}

function runningTools() {
  const set = new Set();
  if (S.st) for (const t of S.st.terminals) if (t && t.pos > 0 && t.state === 'running') set.add(t.title);
  return set;
}

// All and Installed are grouped by main category, and each group folds.
// A filter shows every match, folded or not.
const grouped = () => S.tab === 'all' || S.tab === 'installed';
const folding = () => grouped() && !S.filter.trim();
const folded = (i) => folding() && S.collapsed.has(S.tools[i].primary);
/** The tools the keyboard moves through: visible and not folded away. */
const navigable = () => visible().filter((i) => !folded(i));

function toggleGroup(id) {
  if (S.collapsed.has(id)) S.collapsed.delete(id); else S.collapsed.add(id);
  saveCollapsed();
  renderTools();
}

$('collapse-all').addEventListener('click', () => {
  for (const i of visible()) S.collapsed.add(S.tools[i].primary);
  saveCollapsed();
  renderTools();
});
$('expand-all').addEventListener('click', () => {
  S.collapsed.clear();
  saveCollapsed();
  renderTools();
});

function renderTools() {
  const list = $('tools');
  const vis = visible();
  // keep keyboard focus on the same row across the rebuild
  const focusKey = list.contains(document.activeElement) ? document.activeElement.dataset.key : null;
  list.replaceChildren();
  const filtering = !!S.filter.trim();
  const bar = $('list-bar');
  bar.classList.toggle('grouped', folding());
  $('list-count').textContent = `${vis.length} ${vis.length === 1 ? 'tool' : 'tools'}${filtering ? ' match' : ''}`;
  if (!vis.length) {
    const e = document.createElement('div');
    e.className = 'empty';
    const face = document.createElement('span');
    face.className = 'face';
    face.textContent = '∅';
    e.append(face, S.tools.length ? 'Nothing here matches. Try All, or clear the filter.' : 'Loading the catalog…');
    list.append(e);
    return;
  }
  const running = runningTools();
  const counts = new Map();
  for (const i of vis) counts.set(S.tools[i].primary, (counts.get(S.tools[i].primary) || 0) + 1);
  let group = null;
  for (const i of vis) {
    const t = S.tools[i];
    if (grouped() && t.primary !== group) {
      group = t.primary;
      list.append(groupHeader(group, counts.get(group), filtering, vis));
    }
    if (folded(i)) continue;
    const st = running.has(t.name) ? 'running' : status(i);
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'tool';
    b.setAttribute('role', 'option');
    b.setAttribute('aria-selected', String(i === S.sel));
    b.dataset.i = i;
    b.dataset.key = `t:${t.name}`;
    const dot = document.createElement('i');
    dot.className = `dot ${st}`;
    const name = document.createElement('span');
    name.className = 'tool-name';
    name.textContent = t.name;
    const badge = document.createElement('span');
    badge.className = `tool-badge ${st}`;
    badge.textContent = st === 'running' ? '● running' : WORD[st];
    const desc = document.createElement('span');
    desc.className = 'tool-desc';
    desc.textContent = t.description;
    b.append(dot, name, badge, desc);
    b.addEventListener('click', () => select(i));
    b.addEventListener('dblclick', () => select(i, true));
    list.append(b);
  }
  if (focusKey) list.querySelector(`[data-key="${CSS.escape(focusKey)}"]`)?.focus({ preventScroll: true });
}

function groupHeader(id, count, filtering, vis) {
  const open = filtering || !S.collapsed.has(id);
  const g = document.createElement('button');
  g.type = 'button';
  g.className = filtering ? 'group filtering' : 'group';
  g.dataset.key = `g:${id}`;
  g.style.setProperty('--c', String(catHue(id)));
  g.setAttribute('aria-expanded', String(open));
  g.title = filtering ? 'Showing every match while filtering' : `${open ? 'Fold' : 'Unfold'} ${catLabel(id)}`;
  g.innerHTML = '<svg class="chev" viewBox="0 0 24 24" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg><i class="gdot"></i>';
  const label = document.createElement('span');
  label.textContent = catLabel(id);
  const line = document.createElement('i');
  line.className = 'gline';
  g.append(label, line);
  // a folded group holding the selected tool says so
  if (!open && vis.includes(S.sel) && S.tools[S.sel].primary === id) {
    const sel = document.createElement('i');
    sel.className = 'gsel';
    sel.title = `${S.tools[S.sel].name} is selected`;
    g.append(sel);
  }
  const n = document.createElement('span');
  n.className = 'gcount';
  n.textContent = count;
  g.append(n);
  if (!filtering) g.addEventListener('click', () => toggleGroup(id));
  return g;
}

function select(i, focusArgs) {
  if (i !== S.sel) {
    S.sel = i;
    store.set('ws-tool', S.tools[i].name);
    for (const el of $('tools').querySelectorAll('.tool')) {
      el.setAttribute('aria-selected', String(Number(el.dataset.i) === i));
    }
    $('tools').querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
    renderCard();
    if (folding()) renderTools(); // a folded group's "selected" dot
  }
  if (focusArgs) $('args').focus();
}

function moveSel(delta) {
  const vis = navigable();
  if (!vis.length) return;
  const pos = vis.indexOf(S.sel);
  const next = pos < 0 ? 0 : Math.max(0, Math.min(vis.length - 1, pos + delta));
  select(vis[next]);
}

function stepTab(delta) {
  const i = S.tabs.findIndex((t) => t.key === S.tab);
  const n = S.tabs.length;
  setTab(S.tabs[(i + delta + n) % n].key);
}

$('filter').addEventListener('input', (e) => {
  S.filter = e.target.value;
  renderCats();
  renderTools();
  const vis = visible();
  if (vis.length && !vis.includes(S.sel)) select(vis[0]);
});
$('filter').addEventListener('keydown', (e) => {
  if (e.key === 'ArrowDown') { e.preventDefault(); moveSel(1); }
  else if (e.key === 'ArrowUp') { e.preventDefault(); moveSel(-1); }
  else if (e.key === 'Enter') { e.preventDefault(); const v = visible(); if (v.length) select(v.includes(S.sel) ? S.sel : v[0], true); }
  else if (e.key === 'Escape') { e.target.value = ''; S.filter = ''; renderCats(); renderTools(); e.target.blur(); }
});

// ------------------------------------------------------------- tool card

function renderStats() {
  const count = (s) => S.st.status.filter((x) => x === s).length;
  $('n-installed').textContent = count('installed');
  $('n-missing').textContent = count('missing');
  const c = count('conflict');
  $('n-conflict').textContent = c;
  $('stat-conflict').hidden = c === 0;
  const r = S.st.terminals.filter((t) => t && t.state === 'running').length;
  $('n-running').textContent = r;
  $('stat-running').hidden = r === 0;
  const re = document.querySelector('[data-action="redetect"]');
  re.classList.toggle('busy', !!S.st.detecting);
  re.querySelector('span').textContent = S.st.detecting ? 'Detecting…' : 'Re-detect';
}

function renderCard() {
  const t = S.tools[S.sel];
  if (!t) return;
  const s = status(S.sel);
  const st = S.st || {};
  const [launchLabel, verb] = LAUNCH[t.launch] || LAUNCH.stream;

  $('t-name').textContent = t.name;
  $('t-desc').textContent = t.description;
  const chips = $('t-cats');
  chips.replaceChildren(...t.categories.map((c) => {
    const el = document.createElement('span');
    el.className = c === t.primary ? 'chip primary' : 'chip';
    if (c === t.primary) el.style.setProperty('--c', String(catHue(c)));
    el.textContent = catLabel(c);
    return el;
  }));
  const kind = document.createElement('span');
  kind.className = 'chip';
  kind.textContent = launchLabel;
  chips.append(kind);
  const badge = $('t-status');
  badge.className = `badge ${s}`;
  badge.textContent = WORD[s];

  $('t-note').hidden = !t.note;
  $('t-note').textContent = t.note ? `Note: ${t.note}` : '';

  const conflict = st.conflicts && st.conflicts[S.sel];
  $('t-conflict').hidden = s !== 'conflict';
  if (s === 'conflict') {
    const code = document.createElement('code');
    code.textContent = conflict || '';
    $('t-conflict').replaceChildren(
      `A different program named "${t.name}" comes first on PATH: `, code,
      '. Install the right one, or move it later in PATH, then re-detect.');
  }

  // args: refill when the tool changes (last used, else the example)
  const history = (st.history && st.history[t.name]) || [];
  if (S.argsFor !== t.name) {
    S.argsFor = t.name;
    $('args').value = history.length ? history[history.length - 1] : (t.args_hint || '');
    closeSuggest();
  }
  // recipes: filled from the active challenge; {target}-style gaps are
  // left for you to replace
  const recipes = (st.recipes && st.recipes[t.name]) || [];
  const rsel = $('recipe');
  const rsig = JSON.stringify([t.name, recipes]);
  if (rsel.dataset.sig !== rsig) {
    rsel.dataset.sig = rsig;
    rsel.replaceChildren(new Option('Recipe…', ''), ...recipes.map((r, i) => new Option(r.name, String(i))));
  }
  rsel.hidden = recipes.length === 0 || t.launch === 'dataset';
  $('t-cmd').textContent = `$ ${t.run_cmd || t.name}`;
  $('args-wrap').hidden = t.launch === 'dataset' || (t.launch === 'web' && !t.run_cmd);
  $('args').placeholder = (t.options || []).length ? 'arguments · type - for flags' : 'arguments';
  renderExplain();
  const hint = $('args-hint');
  hint.replaceChildren();
  if (t.args_hint && t.launch !== 'dataset') {
    const code = document.createElement('code');
    code.textContent = t.args_hint;
    hint.append('Example: ', code);
  } else if (t.launch === 'dataset') {
    hint.textContent = 'Lists the files in the console and copies the folder path on the machine running winsploit.';
  }

  const run = $('run-btn');
  run.textContent = verb;
  run.disabled = s !== 'installed';
  run.title = s === 'installed' ? `${verb} ${t.name} in a free terminal` : s === 'unknown' ? 'Still detecting…' : 'Install it first';

  const inst = $('install-btn');
  inst.hidden = !(s === 'missing' || s === 'conflict');
  inst.textContent = `Install with ${t.install}`;
  inst.disabled = !!(st.terminals && st.terminals[0] && st.terminals[0].state === 'running');
  inst.classList.toggle('primary', s === 'missing');
}

// ------------------------------------------------------------- terminals

const SPIN = '⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏';
let spinTick = 0;

function renderTabs() {
  const bar = $('term-tabs');
  bar.replaceChildren();
  const terms = S.st ? S.st.terminals : [];
  for (let pos = 0; pos <= S.slots; pos++) {
    const t = terms[pos];
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'term-tab';
    b.setAttribute('role', 'tab');
    b.setAttribute('aria-selected', String(S.grid ? false : pos === S.focus));
    if (!t && pos > 0) b.classList.add('free');
    const num = document.createElement('span');
    num.className = 'num';
    num.textContent = pos;
    b.append(num, pos === 0 ? 'console' : t ? t.title : 'free');
    if (t) {
      const st = document.createElement('span');
      st.className = 'st';
      if (t.state === 'running') {
        st.classList.add('running');
        st.textContent = SPIN[spinTick % SPIN.length];
      } else if (t.state === 'exited' || t.state === 'killed') {
        st.classList.add(t.failed ? 'err' : 'ok');
        st.textContent = t.failed ? '✗' : '✓';
      }
      if (st.textContent) b.append(st);
    }
    if (S.unread.has(pos)) {
      const u = document.createElement('i');
      u.className = 'unread';
      b.append(u);
    }
    b.title = pos === 0 ? 'Console: installs, hash ids, messages (0)' : t ? `${t.title} (${pos})` : `Terminal ${pos} is free (${pos})`;
    b.addEventListener('click', () => setFocus(pos, true));
    bar.append(b);
  }
  const sp = document.createElement('div');
  sp.className = 'spacer';
  const g = document.createElement('button');
  g.type = 'button';
  g.className = `btn sm ghost grid-btn${S.grid ? ' on' : ''}`;
  g.title = 'All four terminals at once (g)';
  g.textContent = S.grid ? '▣ Grid' : '▦ Grid';
  g.addEventListener('click', toggleGrid);
  bar.append(sp, g);
}
setInterval(() => {
  if (S.st && S.st.terminals.some((t) => t && t.state === 'running')) {
    spinTick++;
    for (const el of document.querySelectorAll('.term-tab .st.running')) el.textContent = SPIN[spinTick % SPIN.length];
  }
}, 120);

function setFocus(pos, leaveGrid) {
  if (pos === 0 || leaveGrid) S.grid = false;
  S.focus = pos;
  S.unread.delete(pos);
  renderTerminals();
}

function toggleGrid() {
  S.grid = !S.grid;
  if (S.grid && S.focus === 0) S.focus = 1;
  if (S.grid) for (let p = 1; p <= S.slots; p++) S.unread.delete(p);
  renderTerminals();
}

function renderTerminals() {
  if (!S.grid) S.unread.delete(S.focus);
  renderTabs();
  const body = $('term-body');
  body.classList.toggle('grid', S.grid);
  $('split-gx').hidden = !S.grid;
  $('split-gy').hidden = !S.grid;
  const want = S.grid ? Array.from({ length: S.slots }, (_, i) => i + 1) : [S.focus];
  const nodes = want.map((pos) => paneView(pos));
  // only touch the DOM when the arrangement changed (keeps scroll and focus)
  const same = nodes.length === body.children.length && nodes.every((n, i) => body.children[i] === n);
  if (!same) body.replaceChildren(...nodes);
  if (S.find.text) refreshFind(false);
}

// The element for terminal `pos`: its pane, or a "free" placeholder.
function paneView(pos) {
  const t = S.st && S.st.terminals[pos];
  if (!t) return freePane(pos);
  const pane = paneFor(pos);
  pane.classList.toggle('focus', S.grid && pos === S.focus);
  // the terminal's output element moves into this pane
  const out = term(t.sid).el;
  const cur = pane.querySelector('.out');
  if (cur !== out) {
    cur.replaceWith(out);
    if (term(t.sid).follow) requestAnimationFrame(() => { out.scrollTop = out.scrollHeight; });
  }

  const rs = pane.querySelector('.run-state');
  let text;
  let state = 'idle';
  if (t.state === 'running') {
    state = 'running';
    const what = pos === 0 ? 'Installing…' : `Running ${t.title}`;
    text = t.pid ? `${what} · pid ${t.pid}` : what;
  } else if (t.state === 'exited') {
    state = t.failed ? 'err' : 'ok';
    text = `${pos === 0 ? 'Install' : t.title} exited (code ${t.code ?? '?'})`;
  } else if (t.state === 'killed') {
    state = 'err';
    text = `${t.title} stopped`;
  } else {
    text = pos === 0 ? 'Console · installs, hash ids, messages' : t.title;
  }
  rs.dataset.state = state;
  rs.querySelector('.run-text').textContent = text;
  pane.querySelector('[data-action="cancel"]').disabled = t.state !== 'running';
  pane.querySelector('.close-btn').hidden = pos === 0;
  const log = pane.querySelector('.log-link');
  log.hidden = !t.log;
  if (t.log) {
    log.href = `/api/log?sid=${t.sid}`;
    log.setAttribute('download', '');
  }
  const stdin = pane.querySelector('.stdin');
  const wasHidden = stdin.hidden;
  stdin.hidden = !(t.state === 'running' && t.stdin);
  pane.querySelector('.stdin-input').placeholder = `Type a line for ${t.title}`;
  if (wasHidden && !stdin.hidden && term(t.sid).follow) out.scrollTop = out.scrollHeight;
  return pane;
}

const freePanes = new Map();
function freePane(pos) {
  let d = freePanes.get(pos);
  if (d) return d;
  d = document.createElement('div');
  d.className = 'free-pane';
  const art = document.createElement('pre');
  art.textContent = '┌──────────┐\n│ >_       │\n└──────────┘';
  const p = document.createElement('div');
  const b = document.createElement('b');
  b.textContent = `Terminal ${pos} is free.`;
  p.append(b, document.createElement('br'), 'Pick a tool and press Run: it starts here.');
  d.append(art, p);
  freePanes.set(pos, d);
  return d;
}

function paneFor(pos) {
  let pane = S.panes.get(pos);
  if (pane) return pane;
  pane = $('pane-tpl').content.firstElementChild.cloneNode(true);
  const sid = () => S.st.terminals[pos]?.sid;
  pane.addEventListener('mousedown', () => {
    if (S.grid && S.focus !== pos) { S.focus = pos; renderTerminals(); }
  });
  pane.querySelector('[data-action="cancel"]').addEventListener('click', () => act({ type: 'cancel', term: pos }));
  pane.querySelector('[data-action="close"]').addEventListener('click', () => {
    const t = S.st.terminals[pos];
    if (SET.confirmClose && t && t.state === 'running' && !confirm(`${t.title} is still running. Stop it and close terminal ${pos}?`)) return;
    act({ type: 'close', term: pos });
  });
  pane.querySelector('.stdin').addEventListener('submit', (e) => {
    e.preventDefault();
    const input = pane.querySelector('.stdin-input');
    act({ type: 'stdin', term: pos, line: input.value });
    input.value = '';
  });
  pane.querySelector('[data-action="eof"]').addEventListener('click', () => act({ type: 'eof', term: pos }));
  pane.querySelector('.stdin-input').addEventListener('keydown', (e) => {
    if (e.key === 'd' && e.ctrlKey) { e.preventDefault(); act({ type: 'eof', term: pos }); }
  });
  pane.querySelector('.copy-out').addEventListener('click', async () => {
    const t = S.terms.get(sid());
    try {
      await navigator.clipboard.writeText((t ? t.lines : []).join('\n'));
      toast(`Copied ${t ? t.lines.length : 0} lines`);
    } catch {
      toast('Copy failed: the browser blocked clipboard access.', 'err');
    }
  });
  pane.querySelector('.jump').addEventListener('click', (e) => {
    const t = S.terms.get(sid());
    if (t) { t.el.scrollTop = t.el.scrollHeight; t.follow = true; }
    e.currentTarget.hidden = true;
  });
  const find = pane.querySelector('.find-input');
  find.addEventListener('input', () => { S.find.text = find.value; refreshFind(true); });
  find.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); stepFind(!e.shiftKey); }
  });
  pane.querySelector('.find-prev').addEventListener('click', () => stepFind(true));
  pane.querySelector('.find-next').addEventListener('click', () => stepFind(false));
  S.panes.set(pos, pane);
  return pane;
}

// --------------------------------------------------------- command helper
// Suggestions under the arguments box: the catalog's flags (tools.yaml
// `options:`), recipes, this tool's history and the active challenge's
// target and files. It only edits the text; running stays on Enter / Run.

const H = { open: false, items: [], idx: 0, nav: false, head: '' };
const argsEl = $('args');

/** Quote-aware words with their positions: `-w "a b.txt"` is two. */
function lexArgs(v) {
  const out = [];
  const re = /(?:[^\s"]+|"[^"]*"?)+/g;
  let m;
  while ((m = re.exec(v))) out.push({ text: m[0], start: m.index, end: m.index + m[0].length });
  return out;
}

/** The catalog option a word is, if any: `-p`, `--crawl=2`, `--wordlist=x`. */
function findOpt(t, word) {
  const opts = (t && t.options) || [];
  for (const o of opts) {
    if (o.flag === word) return { opt: o, value: null };
    if (o.flag.endsWith('=') && word.startsWith(o.flag)) return { opt: o, value: word.slice(o.flag.length) };
    if (o.flag.startsWith('-') && word.startsWith(`${o.flag}=`)) return { opt: o, value: word.slice(o.flag.length + 1) };
  }
  return null;
}
const isSub = (o) => !o.flag.startsWith('-');
const takesValue = (o) => !!o.arg && !o.flag.endsWith('=');

function challengeValues() {
  const c = S.st && S.st.challenge;
  const out = [];
  if (!c) return out;
  if (c.target) out.push({ value: c.target, desc: 'target of the active challenge', tag: 'target' });
  for (const f of c.files || []) {
    const path = c.files_dir ? `${c.files_dir}\\${f}` : f;
    out.push({ value: path, label: f, desc: 'challenge file', tag: 'file' });
  }
  return out;
}

const quoteArg = (v) => (/[\s"]/.test(v) ? `"${v.replace(/"/g, '')}"` : v);

/** What to offer for the line and caret right now. */
function computeSuggest(force) {
  const t = S.tools[S.sel];
  const items = [];
  let head = '';
  if (!t) return { items, head };
  const v = argsEl.value;
  const caret = argsEl.selectionStart ?? v.length;
  const toks = lexArgs(v);
  const cur = toks.find((x) => x.start <= caret && caret <= x.end) || { text: '', start: caret, end: caret };
  const word = cur.text.slice(0, caret - cur.start);
  const before = toks.filter((x) => x.end <= cur.start);
  const prev = before[before.length - 1];
  const opts = t.options || [];
  if (word.startsWith('"')) return { items, head };
  const range = { start: cur.start, end: cur.end };

  // an empty line: whole commands
  if (!v.trim()) {
    const recipes = (S.st && S.st.recipes && S.st.recipes[t.name]) || [];
    const seen = new Set();
    for (const r of recipes) {
      seen.add(r.args);
      items.push({ sec: 'Recipes', kind: 'line', key: r.args, tag: r.name, line: r.args });
    }
    const history = ((S.st && S.st.history && S.st.history[t.name]) || []).slice().reverse();
    for (const h of history) {
      if (seen.has(h) || !h.trim()) continue;
      seen.add(h);
      items.push({ sec: 'Recent', kind: 'line', key: h, tag: 'used before', line: h });
      if (items.length > 12) break;
    }
    if (t.args_hint && !seen.has(t.args_hint)) items.push({ sec: 'Example', kind: 'line', key: t.args_hint, tag: 'example', line: t.args_hint });
  }

  // the value after a flag that takes one
  const prevOpt = prev && cur.start > prev.end && findOpt(t, prev.text);
  if (prevOpt && takesValue(prevOpt.opt) && prevOpt.value === null && !word.startsWith('-')) {
    head = `${prevOpt.opt.flag} ${prevOpt.opt.arg}: ${prevOpt.opt.help}`;
    for (const c of challengeValues()) {
      if (word && !c.value.toLowerCase().includes(word.toLowerCase()) && !(c.label || '').toLowerCase().includes(word.toLowerCase())) continue;
      items.push({ sec: 'From the challenge', kind: 'value', key: c.label || c.value, desc: c.desc, tag: c.tag, value: c.value, range });
    }
    return { items, head, show: true };
  }

  // flags and subcommands
  const used = new Set(toks.filter((x) => x !== cur).map((x) => {
    const m = findOpt(t, x.text);
    return m ? m.opt.flag : x.text;
  }));
  const hasSub = before.some((x) => opts.some((o) => isSub(o) && o.flag === x.text));
  const w = word.toLowerCase();
  const scored = [];
  for (const o of opts) {
    if (isSub(o) && hasSub) continue;
    const f = o.flag.toLowerCase();
    let score;
    if (!w) score = isSub(o) ? 0 : 1;
    else if (f.startsWith(w)) score = f === w ? -1 : 0;
    else if (w.replace(/^-+/, '').length >= 2 && (f.includes(w.replace(/^-+/, '')) || o.help.toLowerCase().includes(w.replace(/^-+/, '')))) score = 2;
    else continue;
    if (used.has(o.flag)) score += 10;
    scored.push({ o, score });
  }
  scored.sort((a, b) => a.score - b.score);
  for (const { o } of scored) {
    items.push({ sec: isSub(o) ? 'Subcommands' : 'Flags', kind: 'opt', key: o.flag, arg: o.arg, desc: o.help, used: used.has(o.flag), opt: o, range, word });
  }
  // a plain word that isn't a flag: maybe the target or a file
  if (w && !w.startsWith('-')) {
    for (const c of challengeValues()) {
      if (!c.value.toLowerCase().includes(w) && !(c.label || '').toLowerCase().includes(w)) continue;
      items.push({ sec: 'From the challenge', kind: 'value', key: c.label || c.value, desc: c.desc, tag: c.tag, value: c.value, range });
    }
  }
  // typing a word shows matches; anywhere else only when asked for
  const show = force || !v.trim() || (!!w && items.length > 0);
  return { items: items.slice(0, 60), head, show };
}

function updateSuggest(force) {
  if (document.activeElement !== argsEl && !force) return;
  if (!force && !SET.suggest) { closeSuggest(); return; }
  const r = computeSuggest(force);
  if (!r.show || (!r.items.length && !r.head)) { closeSuggest(); return; }
  // keep the chosen row chosen while the list refreshes
  const was = H.open && H.nav ? H.items[H.idx] : null;
  const keep = was ? r.items.findIndex((x) => x.key === was.key && x.sec === was.sec) : -1;
  H.items = r.items;
  H.head = r.head;
  H.idx = keep >= 0 ? keep : 0;
  H.nav = keep >= 0;
  H.open = true;
  renderSuggest();
}

function closeSuggest() {
  if (!H.open) return;
  H.open = false;
  H.nav = false;
  $('suggest').hidden = true;
  argsEl.setAttribute('aria-expanded', 'false');
  argsEl.removeAttribute('aria-activedescendant');
}

function placeSuggest() {
  const box = $('suggest');
  const r = $('args-wrap').getBoundingClientRect();
  const below = window.innerHeight - r.bottom - 16;
  const above = r.top - 16;
  const up = below < 220 && above > below;
  box.style.setProperty('left', `${Math.max(8, r.left)}px`);
  box.style.setProperty('width', `${Math.min(r.width, window.innerWidth - 16)}px`);
  if (up) {
    box.style.setProperty('top', 'auto');
    box.style.setProperty('bottom', `${window.innerHeight - r.top + 6}px`);
  } else {
    box.style.setProperty('bottom', 'auto');
    box.style.setProperty('top', `${r.bottom + 6}px`);
  }
  $('suggest-list').style.setProperty('max-height', `${Math.max(120, Math.min(320, (up ? above : below) - 90))}px`);
}

function renderSuggest() {
  const box = $('suggest');
  const head = $('suggest-head');
  head.replaceChildren();
  if (H.head) {
    const [flag, ...rest] = H.head.split(' ');
    const code = document.createElement('code');
    code.textContent = flag;
    head.append(code, ` ${rest.join(' ')}`);
    if (!H.items.length) head.append(' · type the value');
  }
  const list = $('suggest-list');
  list.replaceChildren();
  let sec = null;
  H.items.forEach((it, i) => {
    if (it.sec !== sec) {
      sec = it.sec;
      const s = document.createElement('div');
      s.className = 'sg-sec';
      s.textContent = sec;
      list.append(s);
    }
    const row = document.createElement('div');
    row.className = `sg ${it.kind === 'line' ? 'line' : it.kind}${it.used ? ' used' : ''}`;
    row.id = `sg-${i}`;
    row.setAttribute('role', 'option');
    row.setAttribute('aria-selected', String(i === H.idx));
    const k = document.createElement('span');
    k.className = 'k';
    const w = it.word || '';
    if (it.kind === 'opt' && w && it.key.toLowerCase().startsWith(w.toLowerCase())) {
      const m = document.createElement('mark');
      m.textContent = it.key.slice(0, w.length);
      k.append(m, it.key.slice(w.length));
    } else {
      k.textContent = it.key;
    }
    if (it.arg) {
      const a = document.createElement('span');
      a.className = 'a';
      a.textContent = it.opt.flag.endsWith('=') ? it.arg : ` ${it.arg}`;
      k.append(a);
    }
    k.title = it.kind === 'value' ? it.value : it.key;
    row.append(k);
    if (it.kind !== 'line') {
      const d = document.createElement('span');
      d.className = 'd';
      d.textContent = it.used ? `${it.desc} · already in the line` : it.desc;
      row.append(d);
    }
    const tag = document.createElement('span');
    tag.className = 't';
    tag.textContent = it.tag || '';
    row.append(tag);
    row.addEventListener('mousedown', (e) => { e.preventDefault(); acceptSuggest(i); });
    row.addEventListener('mousemove', () => {
      if (H.idx === i) return;
      H.idx = i;
      H.nav = true;
      markSuggest();
    });
    list.append(row);
  });
  box.hidden = false;
  argsEl.setAttribute('aria-expanded', 'true');
  placeSuggest();
  markSuggest();
}

function markSuggest() {
  for (const el of $('suggest-list').querySelectorAll('.sg')) {
    el.setAttribute('aria-selected', String(el.id === `sg-${H.idx}`));
  }
  const cur = $(`sg-${H.idx}`);
  if (cur) {
    argsEl.setAttribute('aria-activedescendant', cur.id);
    cur.scrollIntoView({ block: 'nearest' });
  }
}

/** Put text over [start, end) of the line and leave the caret after it. */
function spliceArgs(start, end, text) {
  const v = argsEl.value;
  let rest = v.slice(end);
  if (text.endsWith(' ') && rest.startsWith(' ')) rest = rest.slice(1);
  argsEl.value = v.slice(0, start) + text + rest;
  const at = start + text.length;
  argsEl.setSelectionRange(at, at);
}

function acceptSuggest(i) {
  const it = H.items[i];
  if (!it) return;
  if (it.kind === 'line') {
    argsEl.value = it.line;
    argsEl.setSelectionRange(it.line.length, it.line.length);
    closeSuggest();
  } else if (it.kind === 'opt') {
    const o = it.opt;
    spliceArgs(it.range.start, it.range.end, o.flag.endsWith('=') ? o.flag : `${o.flag} `);
    closeSuggest();
    // a flag that takes a value: say what goes next
    if (o.arg) updateSuggest(true);
  } else if (it.kind === 'value') {
    spliceArgs(it.range.start, it.range.end, `${quoteArg(it.value)} `);
    closeSuggest();
  }
  renderExplain();
}

argsEl.addEventListener('input', () => { updateSuggest(false); renderExplain(); });
argsEl.addEventListener('focus', () => { if (!argsEl.value.trim()) updateSuggest(false); });
argsEl.addEventListener('blur', closeSuggest);
argsEl.addEventListener('click', () => { if (H.open) updateSuggest(true); });
argsEl.addEventListener('keydown', (e) => {
  if (e.key === ' ' && e.ctrlKey) {
    e.preventDefault();
    if (H.open) closeSuggest(); else updateSuggest(true);
    return;
  }
  if (!H.open) {
    if (e.key === 'ArrowDown' && !e.altKey) { e.preventDefault(); updateSuggest(true); }
    return;
  }
  const n = H.items.length;
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault();
    if (!n) return;
    H.idx = H.nav ? (H.idx + (e.key === 'ArrowDown' ? 1 : -1) + n) % n : (e.key === 'ArrowDown' ? 0 : n - 1);
    H.nav = true;
    markSuggest();
  } else if (e.key === 'Tab' && n) {
    e.preventDefault();
    acceptSuggest(H.idx);
  } else if (e.key === 'Enter' && H.nav && n) {
    e.preventDefault();
    acceptSuggest(H.idx);
  } else if (e.key === 'Enter') {
    closeSuggest();
  } else if (e.key === 'Escape') {
    e.preventDefault();
    e.stopPropagation();
    closeSuggest();
  } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'Home' || e.key === 'End') {
    requestAnimationFrame(() => updateSuggest(false));
  }
});
window.addEventListener('resize', () => { if (H.open) placeSuggest(); });
document.addEventListener('scroll', () => { if (H.open) placeSuggest(); }, true);

/** Under the box: what each known flag in the line does. */
function renderExplain() {
  const box = $('args-explain');
  const t = S.tools[S.sel];
  box.replaceChildren();
  if (!SET.explain || !t || !(t.options || []).length || $('args-wrap').hidden) return;
  const toks = lexArgs(argsEl.value);
  let shown = 0;
  for (let i = 0; i < toks.length && shown < 14; i++) {
    const m = findOpt(t, toks[i].text);
    if (!m) continue;
    const o = m.opt;
    let value = m.value;
    if (takesValue(o) && value === null) {
      const next = toks[i + 1];
      if (next && !findOpt(t, next.text)) { value = next.text; i++; }
    }
    const chip = document.createElement('span');
    const missing = takesValue(o) && !value;
    chip.className = missing ? 'ex warn' : 'ex';
    const code = document.createElement('code');
    code.textContent = o.flag;
    chip.append(code);
    if (value) {
      const v = document.createElement('span');
      v.className = 'v';
      v.textContent = value;
      v.title = value;
      chip.append(v);
    }
    chip.append(missing ? `needs ${o.arg}` : o.help);
    chip.title = `${o.flag}${o.arg ? ` ${o.arg}` : ''}: ${o.help}`;
    box.append(chip);
    shown++;
  }
}

// --------------------------------------------------------------- actions

function busySlots() {
  const terms = S.st ? S.st.terminals.slice(1) : [];
  return terms.length === S.slots && terms.every((t) => t && t.state === 'running');
}

function runSelected(slot) {
  const t = S.tools[S.sel];
  if (!t || S.role === 'watch') return;
  const takesTerminal = t.launch === 'stream' || (t.launch === 'web' && t.run_cmd);
  if (takesTerminal && slot === undefined && busySlots()) {
    pickSlot(t);
    return;
  }
  if (takesTerminal) S.expect = { tool: t.name };
  else if (t.launch === 'dataset' || t.launch === 'gui') setFocus(0);
  act({ type: 'run', tool: t.name, args: $('args').value, slot: slot ?? null });
}

function pickSlot(t) {
  $('slot-tool').textContent = t.name;
  const list = $('slot-list');
  list.replaceChildren();
  for (let pos = 1; pos <= S.slots; pos++) {
    const busy = S.st.terminals[pos];
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'slot-choice';
    const num = document.createElement('span');
    num.className = 'num';
    num.textContent = pos;
    const what = document.createElement('span');
    what.className = 'what';
    what.append(busy.title);
    const small = document.createElement('small');
    small.textContent = busy.pid ? `running · pid ${busy.pid}` : 'running';
    what.append(small);
    b.append(num, what);
    b.addEventListener('click', () => { $('slot-dialog').close(); runSelected(pos); });
    list.append(b);
  }
  $('slot-dialog').showModal();
}

$('run-form').addEventListener('submit', (e) => {
  e.preventDefault();
  if (!$('run-btn').disabled) runSelected();
});
$('install-btn').addEventListener('click', () => {
  const t = S.tools[S.sel];
  if (t) {
    setFocus(0);
    act({ type: 'install', tool: t.name });
  }
});
document.querySelector('[data-action="redetect"]').addEventListener('click', () => act({ type: 'redetect' }));

const hashDialog = $('hash-dialog');
$('open-hash').addEventListener('click', () => { $('hash').value = ''; hashDialog.showModal(); });
$('hash-form').addEventListener('submit', (e) => {
  e.preventDefault();
  const h = $('hash').value.trim();
  if (!h) return;
  hashDialog.close();
  setFocus(0);
  act({ type: 'hashid', hash: h });
});
$('open-help').addEventListener('click', () => $('help-dialog').showModal());
for (const b of document.querySelectorAll('[data-close]')) {
  b.addEventListener('click', () => b.closest('dialog').close());
}

// keys: / filter, f find, g grid, 0-4 terminals, j/k list, ←/→ category, b tool list, , settings, ? help
document.addEventListener('keydown', (e) => {
  const typing = /^(INPUT|TEXTAREA)$/.test(document.activeElement?.tagName || '');
  if (e.key === 'Escape' && typing && document.activeElement.id !== 'filter') {
    document.activeElement.blur();
    return;
  }
  if (typing || e.ctrlKey || e.metaKey || e.altKey || document.querySelector('dialog[open]')) return;
  const k = e.key;
  // a focused category header folds on Enter / Space like any button
  if ((k === 'Enter' || k === ' ') && document.activeElement?.classList.contains('group')) return;
  if (k === '/') { e.preventDefault(); $('filter').focus(); $('filter').select(); }
  else if (k === 'f') {
    e.preventDefault();
    if (S.grid) setFocus(S.focus, true);
    const input = S.panes.get(S.focus)?.querySelector('.find-input');
    if (input) { input.focus(); input.select(); }
  }
  else if (k === 'g') { e.preventDefault(); toggleGrid(); }
  else if (k === '?') { e.preventDefault(); $('help-dialog').showModal(); }
  else if (k === 'b') { e.preventDefault(); setSetting('sidebar', !SET.sidebar); }
  else if (k === ',') { e.preventDefault(); openSettings(); }
  else if (/^[0-4]$/.test(k)) { e.preventDefault(); setFocus(Number(k), true); }
  else if (k === 'j' || k === 'ArrowDown') { e.preventDefault(); moveSel(1); }
  else if (k === 'k' || k === 'ArrowUp') { e.preventDefault(); moveSel(-1); }
  else if (k === 'ArrowRight' || k === ']') { e.preventDefault(); stepTab(1); }
  else if (k === 'ArrowLeft' || k === '[') { e.preventDefault(); stepTab(-1); }
  else if (k === 'Enter') { e.preventDefault(); select(S.sel, true); }
});

renderTerminals();
connect();
