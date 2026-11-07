<script setup lang="ts">
// A scripted replay of the real dashboard: browse, switch category, install
// ffuf with i, run it with Enter, run nmap alongside it, then the grid view.
// It plays only while on screen and loops.
const cat = useCatalog();
const SEC = 'C:\\Users\\you\\scoop\\apps\\seclists-lite\\current';
const hint = (name: string) => (cat.tools.find((t) => t.name === name)?.args_hint ?? '').replaceAll('{app:seclists-lite}', SEC);

type St = 'installed' | 'missing' | 'conflict';
const initial = (): Record<string, St> => {
  const s: Record<string, St> = {};
  for (const t of cat.tools) s[t.name] = 'installed';
  for (const n of ['ffuf', 'feroxbuster', 'katana', 'arjun', 'amass', 'wfuzz']) if (n in s) s[n] = 'missing';
  if ('httpx' in s) s.httpx = 'conflict';
  return s;
};

const tabs = [{ id: 'all', label: 'All' }, { id: 'installed', label: 'Installed' }, ...cat.categories.map((c) => ({ id: c.id, label: c.label }))];
interface Term { title: string; lines: string[]; running: boolean }

const S = reactive({
  tab: 0,
  sel: 0,
  status: initial(),
  focus: 0,
  grid: false,
  prompt: null as null | { tool: string; text: string },
  terms: [{ title: 'console', lines: [] as string[], running: false }] as (Term | null)[],
  key: '',
  keyN: 0,
  line: 'ready · Enter runs the selected tool · ? shows every key',
});

const list = computed(() => {
  const id = tabs[S.tab]!.id;
  if (id === 'all') return cat.tools;
  if (id === 'installed') return cat.tools.filter((t) => S.status[t.name] === 'installed');
  return cat.tools.filter((t) => t.categories.includes(id));
});
const VISIBLE = 11;
const scrollTop = computed(() => Math.max(0, Math.min(S.sel - VISIBLE + 3, list.value.length - VISIBLE)));
const rows = computed(() => list.value.slice(scrollTop.value, scrollTop.value + VISIBLE));
const counts = computed(() => {
  const v = Object.values(S.status);
  return { ok: v.filter((x) => x === 'installed').length, miss: v.filter((x) => x === 'missing').length, cf: v.filter((x) => x === 'conflict').length };
});
const glyph = { installed: '●', missing: '○', conflict: '◐' } as const;

// ------------------------------------------------------------- the script
let alive = true;
let visible = false;
// sleeps started while off screen wait here until the window is visible again
let waiting: (() => void)[] = [];
const sleep = (ms: number) =>
  new Promise<void>((done) => {
    const go = () => setTimeout(done, ms);
    if (visible) go();
    else waiting.push(go);
  });

async function press(k: string, ms = 420) {
  S.key = k;
  S.keyN++;
  await sleep(ms);
}
function push(i: number, ...lines: string[]) {
  const t = S.terms[i];
  if (t) t.lines.push(...lines);
}
async function stream(i: number, lines: string[], each = 110) {
  for (const l of lines) {
    if (!alive) return;
    push(i, l);
    await sleep(each + Math.random() * each);
  }
}
async function moveTo(name: string) {
  let idx = list.value.findIndex((t) => t.name === name);
  if (idx < 0) idx = 0;
  while (S.sel !== idx && alive) {
    const down = idx > S.sel;
    S.sel += down ? 1 : -1;
    await press(down ? '↓' : '↑', 170);
  }
}
async function tabTo(id: string) {
  const want = tabs.findIndex((t) => t.id === id);
  while (S.tab !== want && alive) {
    const right = want > S.tab;
    S.tab += right ? 1 : -1;
    S.sel = 0;
    await press(right ? '→' : '←', 330);
  }
}
async function type(text: string) {
  S.prompt!.text = '';
  for (const ch of text) {
    S.prompt!.text += ch;
    await sleep(12);
  }
}

const FFUF_OUT = [
  '',
  '        /\'___\\  /\'___\\           /\'___\\',
  '       /\\ \\__/ /\\ \\__/  __  __  /\\ \\__/',
  '       \\ \\ ,__\\\\ \\ ,__\\/\\ \\/\\ \\ \\ \\ ,__\\',
  '        \\ \\_\\/ \\ \\_\\/\\ \\ \\_\\ \\ \\ \\_\\/',
  '         \\/_/   \\/_/  \\/___/   \\/_/       v2.1.0',
  '________________________________________________',
  ' :: Method           : GET',
  ' :: URL              : https://example.com/FUZZ',
  ' :: Wordlist         : FUZZ: …\\seclists-lite\\current\\common.txt',
  ' :: Threads          : 40',
  '________________________________________________',
  'admin                   [Status: 301, Size: 169, Words: 5, Lines: 8]',
  'assets                  [Status: 301, Size: 169, Words: 5, Lines: 8]',
  'api                     [Status: 401, Size: 52, Words: 4, Lines: 1]',
  'backup                  [Status: 403, Size: 153, Words: 3, Lines: 7]',
  'index.html              [Status: 200, Size: 1256, Words: 298, Lines: 47]',
  'login                   [Status: 200, Size: 4127, Words: 811, Lines: 96]',
  'robots.txt              [Status: 200, Size: 64, Words: 6, Lines: 4]',
  ':: Progress: [4750/4750] :: 312 req/sec :: Duration: [0:00:15] ::',
  '── process exited (code 0) ──',
];
const NMAP_OUT = [
  'Starting Nmap 7.95 ( https://nmap.org )',
  'Nmap scan report for scanme.nmap.org (45.33.32.156)',
  'Host is up (0.16s latency).',
  'Not shown: 996 closed tcp ports (reset)',
  'PORT      STATE SERVICE    VERSION',
  '22/tcp    open  ssh        OpenSSH 6.6.1p1 Ubuntu 2ubuntu2.13',
  '80/tcp    open  http       Apache httpd 2.4.7 ((Ubuntu))',
  '9929/tcp  open  nping-echo Nping echo',
  '31337/tcp open  tcpwrapped',
  'Service Info: OS: Linux; CPE: cpe:/o:linux:linux_kernel',
  'Nmap done: 1 IP address (1 host up) scanned in 14.82 seconds',
  '── process exited (code 0) ──',
];

function reset() {
  S.tab = 0;
  S.sel = 0;
  S.status = initial();
  S.focus = 0;
  S.grid = false;
  S.prompt = null;
  S.terms = [
    {
      title: 'console',
      running: false,
      lines: [
        ':: winsploit ready. Pick a tool on the left; ← → switch category.',
        `:: Enter runs it in its own terminal (up to ${cat.slots} at once) · i installs it.`,
        ':: catalog: C:\\Tools\\winsploit\\tools.yaml',
      ],
    },
    null,
    null,
    null,
    null,
  ];
  S.line = 'ready · Enter runs the selected tool · ? shows every key';
}

async function runTool(name: string, slot: number, out: string[]) {
  const args = hint(name);
  await press('⏎', 300);
  S.prompt = { tool: name, text: '' };
  await type(args);
  await sleep(700);
  await press('⏎', 200);
  S.prompt = null;
  const t = cat.tools.find((x) => x.name === name)!;
  S.terms[slot] = { title: name, running: true, lines: [`$ ${t.run_cmd} ${args}`] };
  S.focus = slot;
  S.line = `running ${name} in terminal ${slot} (pid ${4000 + slot * 1373})`;
  return stream(slot, out, name === 'nmap' ? 260 : 120).then(() => {
    const term = S.terms[slot];
    if (term) term.running = false;
  });
}

async function play() {
  reset();
  await sleep(900);
  await moveTo(list.value[4]!.name);
  await sleep(300);
  await tabTo('web');
  await sleep(250);
  await moveTo('ffuf');
  S.line = 'ffuf is not installed — press i to install it';
  await sleep(800);
  // install
  await press('i', 300);
  S.focus = 0;
  push(0, '$ scoop bucket add security https://github.com/opsqw/scoop-security && scoop install security/ffuf');
  await stream(0, [
    "Checking repo... OK",
    "The security bucket was added successfully.",
    "Installing 'ffuf' (2.1.0) [64bit] from 'security' bucket",
    "ffuf_2.1.0_windows_amd64.zip (3.9 MB) [=============================] 100%",
    "Checking hash of ffuf_2.1.0_windows_amd64.zip ... ok.",
    "Linking ~\\scoop\\apps\\ffuf\\current => ~\\scoop\\apps\\ffuf\\2.1.0",
    "Creating shim for 'ffuf'.",
    "'ffuf' (2.1.0) was installed successfully!",
    "── process exited (code 0) ──",
  ], 150);
  S.status.ffuf = 'installed';
  S.line = 'ffuf installed · PATH refreshed';
  await sleep(900);
  // run ffuf in terminal 1, then nmap in terminal 2 while ffuf still runs
  const ffuf = runTool('ffuf', 1, FFUF_OUT);
  await sleep(2600);
  await tabTo('network');
  await moveTo('nmap');
  const nmap = runTool('nmap', 2, NMAP_OUT);
  await sleep(2400);
  await press('g', 300);
  S.grid = true;
  S.line = 'grid: all four terminals at once · g again for one';
  await Promise.all([ffuf, nmap]);
  await sleep(3200);
}

const win = ref<HTMLElement>();
onMounted(() => {
  const io = new IntersectionObserver(([e]) => {
    visible = !!e?.isIntersecting;
    if (visible && waiting.length) {
      const w = waiting;
      waiting = [];
      w.forEach((go) => go());
    }
  }, { threshold: 0.25 });
  io.observe(win.value!);
  if (prefersReducedMotion()) {
    // a still frame of the finished state
    reset();
    S.tab = tabs.findIndex((t) => t.id === 'network');
    S.sel = Math.max(0, list.value.findIndex((t) => t.name === 'nmap'));
    S.terms[1] = { title: 'ffuf', running: false, lines: [`$ ffuf ${hint('ffuf')}`, ...FFUF_OUT.slice(11)] };
    S.terms[2] = { title: 'nmap', running: false, lines: [`$ nmap ${hint('nmap')}`, ...NMAP_OUT] };
    S.grid = true;
    return;
  }
  (async () => {
    while (alive) await play();
  })();
  onBeforeUnmount(() => {
    alive = false;
    io.disconnect();
  });
});

const shownTerms = computed(() =>
  S.grid ? [1, 2, 3, 4].map((i) => ({ i, t: S.terms[i] })) : [{ i: S.focus, t: S.terms[S.focus] }],
);
const lineCls = (l: string) =>
  l.startsWith('$ ') ? 'l-cmd' : l.startsWith('::') || l.startsWith(' ::') || l.startsWith('──') ? 'l-meta' : l.startsWith('[err]') ? 'l-err' : /\[Status: 2/.test(l) || /\bopen\b/.test(l) ? 'l-hit' : '';
</script>

<template>
  <div ref="win" class="window tui" role="img" aria-label="Replay of the Winsploit dashboard: installing ffuf, then running ffuf and nmap in two terminals">
    <div class="window__bar">
      <span class="window__dots"><i /><i /><i /></span>
      <span>winsploit — Windows Terminal</span>
    </div>
    <div class="tui__body">
      <div class="tui__head">
        <span class="tui__brand">WINSPLOIT</span>
        <span class="t-ok">● {{ counts.ok }} ready</span>
        <span class="t-dim">○ {{ counts.miss }} to get</span>
        <span v-if="counts.cf" class="t-err">◐ {{ counts.cf }} PATH</span>
      </div>
      <div class="tui__tabs">
        <span v-for="(t, i) in tabs" :key="t.id" :class="{ 'is-on': i === S.tab }">{{ t.label }}</span>
      </div>
      <div class="tui__main">
        <ul class="tui__list">
          <li v-for="t in rows" :key="t.name" :class="{ 'is-sel': t.name === list[S.sel]?.name }">
            <span :class="`g-${S.status[t.name]}`">{{ glyph[S.status[t.name]!] }}</span>
            <span class="tui__name">{{ t.name }}</span>
          </li>
        </ul>
        <div class="tui__right">
          <div class="tui__ttabs">
            <span v-for="(t, i) in S.terms" :key="i" :class="{ 'is-on': !S.grid && i === S.focus, 'is-free': !t }">
              {{ i }} {{ t ? t.title : 'free' }}<b v-if="t?.running" class="tui__spin">▸</b>
            </span>
          </div>
          <div class="tui__panes" :class="{ 'is-grid': S.grid }">
            <div v-for="p in shownTerms" :key="p.i" class="tui__pane">
              <div v-if="S.grid" class="tui__ptitle">{{ p.i }} {{ p.t ? p.t.title : 'free' }}</div>
              <div class="tui__out">
                <template v-if="p.t">
                  <div v-for="(l, k) in p.t.lines.slice(S.grid ? -9 : -15)" :key="p.t.lines.length - k" :class="lineCls(l)">{{ l || ' ' }}</div>
                </template>
                <div v-else class="t-dim">— free —</div>
              </div>
            </div>
          </div>
        </div>
      </div>
      <div v-if="S.prompt" class="tui__prompt">
        <span class="t-ok">{{ S.prompt.tool }} ›</span> <span>{{ S.prompt.text }}</span><i class="tui__caret" />
      </div>
      <div class="tui__status">{{ S.line }}</div>
      <div class="tui__foot">
        <span><b>⏎</b> run</span><span><b>i</b> install</span><span><b>/</b> filter</span><span><b>g</b> grid</span><span><b>?</b> keys</span><span><b>q</b> quit</span>
      </div>
      <Transition name="key">
        <kbd v-if="S.key" :key="S.keyN" class="tui__key">{{ S.key }}</kbd>
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.tui { position: relative; font-family: var(--font-mono); font-size: 12.5px; line-height: 1.55; }
.tui__body { position: relative; padding: 12px 14px 10px; display: grid; gap: 8px; height: 520px; grid-template-rows: auto auto 1fr auto auto auto; }
.tui__head { display: flex; gap: 16px; align-items: center; white-space: nowrap; overflow: hidden; }
.tui__brand { font-family: var(--font-pixel); color: var(--accent); letter-spacing: .08em; margin-right: 6px; }
.tui__tabs { display: flex; gap: 4px; white-space: nowrap; overflow: hidden; mask-image: linear-gradient(90deg, #000 85%, transparent); color: var(--text-3); }
.tui__tabs span { padding: 0 8px; border-radius: 2px; transition: background-color .25s, color .25s; }
.tui__tabs .is-on { background: var(--accent); color: var(--accent-ink); }
.tui__main { display: grid; grid-template-columns: minmax(130px, 32%) 1fr; gap: 10px; min-height: 0; }
.tui__list { list-style: none; margin: 0; padding: 6px 0; border: 1px solid var(--line-strong); border-radius: 6px; overflow: hidden; }
.tui__list li { display: flex; gap: 8px; padding: 0 10px; position: relative; white-space: nowrap; transition: background-color .2s; }
.tui__list li.is-sel { background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--lg-50); }
.tui__list li.is-sel::before { content: ''; position: absolute; left: 0; top: 0; bottom: 0; width: 2px; background: var(--accent); box-shadow: 0 0 10px var(--accent); }
.tui__name { overflow: hidden; text-overflow: ellipsis; }
.g-installed { color: var(--accent); } .g-missing { color: var(--text-3); } .g-conflict { color: var(--risk); }
.tui__right { display: grid; grid-template-rows: auto 1fr; gap: 6px; min-height: 0; min-width: 0; }
.tui__ttabs { display: flex; gap: 6px; white-space: nowrap; overflow: hidden; font-size: 11.5px; }
.tui__ttabs span { padding: 0 8px; border: 1px solid var(--line); border-radius: 2px; color: var(--text-2); }
.tui__ttabs .is-on { border-color: var(--accent); color: var(--accent); }
.tui__ttabs .is-free { color: var(--text-3); border-style: dashed; }
.tui__spin { margin-left: 6px; color: var(--accent); animation: blink 1s steps(2) infinite; font-weight: 400; }
.tui__panes { display: grid; min-height: 0; gap: 6px; }
.tui__panes.is-grid { grid-template-columns: 1fr 1fr; grid-template-rows: 1fr 1fr; }
.tui__pane { border: 1px solid var(--line-strong); border-radius: 6px; padding: 6px 10px; min-height: 0; min-width: 0; overflow: hidden; display: flex; flex-direction: column; }
.tui__ptitle { font-size: 11px; color: var(--accent); border-bottom: 1px solid var(--line); margin-bottom: 4px; }
.tui__out { display: flex; flex-direction: column; justify-content: flex-end; flex: 1; min-height: 0; overflow: hidden; white-space: pre; color: var(--lg-300); }
.is-grid .tui__out { font-size: 10.5px; }
.l-cmd { color: var(--accent); } .l-meta { color: var(--text-3); } .l-err { color: var(--risk); } .l-hit { color: var(--lg-50); }
.tui__prompt { padding: 4px 10px; border: 1px solid var(--accent); border-radius: 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.tui__caret { display: inline-block; width: 7px; height: 14px; background: var(--accent); vertical-align: -2px; animation: blink 1s steps(2) infinite; }
.tui__status { color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.tui__foot { display: flex; gap: 14px; color: var(--text-3); white-space: nowrap; overflow: hidden; font-size: 11.5px; }
.tui__foot b { color: var(--lg-300); font-weight: 500; }
.tui__key {
  position: absolute; right: 18px; bottom: 54px; min-width: 46px; height: 46px; padding: 0 12px; display: grid; place-items: center;
  font-family: var(--font-mono); font-size: 18px; color: var(--accent-ink); background: var(--accent); border-radius: 6px;
  box-shadow: 0 4px 0 var(--bg-800), 0 0 40px var(--accent-glow);
}
.key-enter-active { transition: transform .35s var(--ease-out), opacity .35s; }
.key-leave-active { transition: opacity .3s .1s; position: absolute; }
.key-enter-from { transform: translateY(8px) scale(.8); opacity: 0; }
.key-leave-to { opacity: 0; }
@keyframes blink { 50% { opacity: 0; } }
@media (max-width: 640px) {
  .tui { font-size: 10.5px; }
  .tui__body { height: 440px; padding: 10px; }
  .tui__main { grid-template-columns: 1fr; }
  .tui__list { display: none; }
}
</style>
