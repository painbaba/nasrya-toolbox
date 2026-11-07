// Winsploit web view. The terminal owns the session: this page renders the
// state it streams (/events) and sends actions back (/api/action). The tool
// selection, category, filter, shown terminal, grid, scroll and find are
// local to this tab.
'use strict';

const $ = (id) => document.getElementById(id);
const MAX_LINES = 5000;

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

let theme = store.get('ws-theme') || 'auto';
function applyTheme() {
  const root = document.documentElement;
  if (theme === 'auto') delete root.dataset.theme; else root.dataset.theme = theme;
  const b = $('theme-toggle');
  b.dataset.mode = theme;
  b.title = `Theme: ${theme}`;
}
applyTheme();
$('theme-toggle').addEventListener('click', () => {
  theme = { auto: 'light', light: 'dark', dark: 'auto' }[theme] || 'auto';
  store.set('ws-theme', theme);
  applyTheme();
});

// -------------------------------------------------------- notifications

let notify = store.get('ws-notify') === 'on' && 'Notification' in window && Notification.permission === 'granted';
function renderNotify() {
  const b = $('notify-toggle');
  b.classList.toggle('on', notify);
  b.title = `Desktop notifications: ${notify ? 'on (when this tab is in the background)' : 'off'}`;
}
renderNotify();
$('notify-toggle').addEventListener('click', async () => {
  if (!('Notification' in window)) { toast('This browser has no desktop notifications.', 'err'); return; }
  if (notify) {
    notify = false;
  } else {
    const p = Notification.permission === 'default' ? await Notification.requestPermission() : Notification.permission;
    notify = p === 'granted';
    if (!notify) toast('Notifications are blocked for this page in the browser settings.', 'err');
  }
  store.set('ws-notify', notify ? 'on' : 'off');
  renderNotify();
});

// ----------------------------------------------------------- connection

let es = null;
function connect() {
  es = new EventSource('/events');
  es.addEventListener('hello', (e) => onHello(JSON.parse(e.data)));
  es.addEventListener('state', (e) => onState(JSON.parse(e.data)));
  es.addEventListener('out', (e) => onOut(JSON.parse(e.data)));
  es.addEventListener('toast', (e) => onToast(JSON.parse(e.data)));
  es.addEventListener('end', () => {
    es.close();
    setConn('ended');
    ended('Session ended',
      'The terminal ended this session: it quit, or the web view was turned off with w. ' +
      'Press o in winsploit to open a new one.');
  });
  es.onopen = () => setConn('live');
  es.onerror = () => {
    if (es.readyState === EventSource.CLOSED) {
      setConn('ended');
      ended('Not signed in',
        'This tab has no valid session. Press o in winsploit to open the web view with a fresh link.');
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
  const extra = t.lines.length - MAX_LINES;
  if (extra > 0) {
    t.lines.splice(0, extra);
    for (let i = 0; i < extra; i++) t.el.firstChild?.remove();
  }
  const pos = posOf(o.sid);
  if (pos !== null && o.lines.length && !shown(pos)) {
    S.unread.add(pos);
    renderTabs();
  }
  if (pos === S.focus && S.find.text) refreshFind(false);
  if (t.follow) t.el.scrollTop = t.el.scrollHeight;
  else if (o.lines.length) t.el.parentElement?.querySelector('.jump')?.removeAttribute('hidden');
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
    b.append(`${ICONS[id] || '◇'} ${tab.label}`);
    const s = document.createElement('span');
    s.className = 'n';
    s.textContent = visibleIn(tab.key).length;
    b.append(s);
    b.addEventListener('click', () => setTab(tab.key));
    nav.append(b);
  }
  $('cat-blurb').textContent = S.tabs.find((t) => t.key === S.tab)?.blurb || '';
  nav.querySelector('[aria-pressed="true"]')?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
}

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

function renderTools() {
  const list = $('tools');
  const vis = visible();
  list.replaceChildren();
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
  let group = null;
  for (const i of vis) {
    const t = S.tools[i];
    if (S.tab === 'all' && t.primary !== group) {
      group = t.primary;
      const g = document.createElement('div');
      g.className = 'group';
      g.textContent = catLabel(group);
      list.append(g);
    }
    const st = running.has(t.name) ? 'running' : status(i);
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'tool';
    b.setAttribute('role', 'option');
    b.setAttribute('aria-selected', String(i === S.sel));
    b.dataset.i = i;
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
  }
  if (focusArgs) $('args').focus();
}

function moveSel(delta) {
  const vis = visible();
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
  }
  $('args-history').replaceChildren(...history.slice().reverse().map((h) => {
    const o = document.createElement('option');
    o.value = h;
    return o;
  }));
  $('t-cmd').textContent = `$ ${t.run_cmd || t.name}`;
  $('args-wrap').hidden = t.launch === 'dataset' || (t.launch === 'web' && !t.run_cmd);
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
    if (t && t.state === 'running' && !confirm(`${t.title} is still running. Stop it and close terminal ${pos}?`)) return;
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

// --------------------------------------------------------------- actions

function busySlots() {
  const terms = S.st ? S.st.terminals.slice(1) : [];
  return terms.length === S.slots && terms.every((t) => t && t.state === 'running');
}

function runSelected(slot) {
  const t = S.tools[S.sel];
  if (!t) return;
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

// keys: / filter, f find, g grid, 0-4 terminals, j/k list, ←/→ category, ? help
document.addEventListener('keydown', (e) => {
  const typing = /^(INPUT|TEXTAREA)$/.test(document.activeElement?.tagName || '');
  if (e.key === 'Escape' && typing && document.activeElement.id !== 'filter') {
    document.activeElement.blur();
    return;
  }
  if (typing || e.ctrlKey || e.metaKey || e.altKey || document.querySelector('dialog[open]')) return;
  const k = e.key;
  if (k === '/') { e.preventDefault(); $('filter').focus(); $('filter').select(); }
  else if (k === 'f') {
    e.preventDefault();
    if (S.grid) setFocus(S.focus, true);
    const input = S.panes.get(S.focus)?.querySelector('.find-input');
    if (input) { input.focus(); input.select(); }
  }
  else if (k === 'g') { e.preventDefault(); toggleGrid(); }
  else if (k === '?') { e.preventDefault(); $('help-dialog').showModal(); }
  else if (/^[0-4]$/.test(k)) { e.preventDefault(); setFocus(Number(k), true); }
  else if (k === 'j' || k === 'ArrowDown') { e.preventDefault(); moveSel(1); }
  else if (k === 'k' || k === 'ArrowUp') { e.preventDefault(); moveSel(-1); }
  else if (k === 'ArrowRight' || k === ']') { e.preventDefault(); stepTab(1); }
  else if (k === 'ArrowLeft' || k === '[') { e.preventDefault(); stepTab(-1); }
  else if (k === 'Enter') { e.preventDefault(); select(S.sel, true); }
});

renderTerminals();
connect();
