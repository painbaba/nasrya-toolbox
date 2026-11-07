// Prebuild step: makes the repo the single source of truth for the site.
//
// 1. ../tools.yaml (+ category labels from ../src/registry.rs, the terminal
//    count from ../src/session.rs and the hash formats from ../src/hashid.rs)
//    -> app/data/catalog.json, so no tool or category count is ever typed by hand.
// 2. The browser UI (../src/web/{index.html,app.css,app.js})
//    -> public/demo/, with absolute paths made relative and the mock server
//    (demo-mock.js) loaded before the app.
import { readFileSync, writeFileSync, mkdirSync, existsSync, copyFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { load as loadYaml } from 'js-yaml';

const here = dirname(fileURLToPath(import.meta.url));
const site = resolve(here, '..');
const repo = resolve(site, '..');
const read = (p) => readFileSync(join(repo, p), 'utf8');

// ------------------------------------------------------------ categories

// `Category { id: "recon", label: "Recon", blurb: "..." }` in registry.rs
const registry = read('src/registry.rs');
const taxonomy = [...registry.matchAll(
  /Category\s*\{\s*id:\s*"([^"]+)",\s*label:\s*"([^"]+)",\s*blurb:\s*"([^"]+)",?\s*\}/g,
)].map(([, id, label, blurb]) => ({ id, label, blurb }));
if (taxonomy.length < 5) throw new Error('sync: could not read CATEGORIES from src/registry.rs');

const labelOf = (id) => taxonomy.find((c) => c.id === id)?.label ?? id[0].toUpperCase() + id.slice(1);
const rankOf = (id) => {
  const i = taxonomy.findIndex((c) => c.id === id);
  return i < 0 ? taxonomy.length : i;
};

// ----------------------------------------------------------------- tools

const doc = loadYaml(read('tools.yaml'));
const tools = doc.tools.map((t) => {
  // same folding as registry.rs: legacy `category:` goes first
  const cats = [];
  if (t.category) cats.push(t.category);
  for (const c of t.categories ?? []) if (!cats.includes(c)) cats.push(c);
  return {
    name: t.name,
    categories: cats,
    primary: cats[0],
    description: t.description ?? '',
    exe: t.exe ?? '',
    run_cmd: t.run_cmd ?? '',
    args_hint: t.args_hint ?? '',
    launch: t.launch ?? 'stream',
    install: t.install ?? 'scoop',
    package: (t.install === 'pipx' ? t.pipx : t.scoop) ?? '',
    bucket: t.bucket ?? (t.install === 'pipx' ? null : 'main'),
    note: t.note ?? null,
    verify: t.verify ?? null,
    url: t.url ?? null,
  };
});

const categories = taxonomy
  .map((c) => ({ ...c, tools: tools.filter((t) => t.categories.includes(c.id)).map((t) => t.name) }))
  .filter((c) => c.tools.length);
// categories the taxonomy doesn't know are listed last, like the dashboard does
for (const t of tools) {
  for (const id of t.categories) {
    if (!categories.some((c) => c.id === id)) {
      categories.push({ id, label: labelOf(id), blurb: '', tools: tools.filter((x) => x.categories.includes(id)).map((x) => x.name) });
    }
  }
}
const fileOrder = [...tools]; // the dashboard keeps file order and sorts with `rank`
tools.sort((a, b) => rankOf(a.primary) - rankOf(b.primary));

// ------------------------------------------------------------ other facts

const slots = Number(/pub const SLOTS: usize = (\d+);/.exec(read('src/session.rs'))?.[1] ?? 4);
const hashSrc = read('src/hashid.rs').split('#[cfg(test)]')[0];
const hashFormats = new Set([...hashSrc.matchAll(/\bc\(\s*"([^"]+)"/g)].map((m) => m[1])).size;
const version = /^version\s*=\s*"([^"]+)"/m.exec(read('Cargo.toml'))?.[1] ?? '0.0.0';
const buckets = Object.keys(doc.buckets ?? {});

const catalog = {
  version,
  slots,
  hashFormats,
  buckets,
  counts: {
    tools: tools.length,
    categories: categories.length,
    pipx: tools.filter((t) => t.install === 'pipx').length,
    scoop: tools.filter((t) => t.install === 'scoop').length,
    gui: tools.filter((t) => t.launch === 'gui').length,
    web: tools.filter((t) => t.launch === 'web').length,
    verified: tools.filter((t) => t.verify).length,
    noted: tools.filter((t) => t.note).length,
    ownBucket: tools.filter((t) => t.bucket === 'winsploit').length,
  },
  categories,
  tools,
};

mkdirSync(join(site, 'app/data'), { recursive: true });
writeFileSync(join(site, 'app/data/catalog.json'), JSON.stringify(catalog, null, 2) + '\n');
console.log(`sync: ${tools.length} tools in ${categories.length} categories, ${hashFormats} hash formats, ${slots} terminals`);

// ------------------------------------------------------------ web-view demo

const live = join(repo, 'src/web');
const from = live;
if (!existsSync(join(live, 'app.js'))) throw new Error('sync: src/web/app.js not found');
const out = join(site, 'public/demo');
mkdirSync(out, { recursive: true });

let html = readFileSync(join(from, 'index.html'), 'utf8')
  .replace('href="/favicon.svg"', 'href="favicon.svg"')
  .replace('href="/app.css"', 'href="app.css"')
  .replace('<script src="/app.js" defer></script>',
    '<link rel="stylesheet" href="demo.css">\n<script src="demo-data.js"></script>\n<script src="demo-mock.js"></script>\n<script src="app.js" defer></script>')
  .replace('<title>Winsploit</title>', '<title>Winsploit web view · live demo</title>')
  .replace('<body>', `<body class="is-demo">
<div class="demo-banner" role="note">
  <span class="demo-pill">DEMO</span>
  <span class="demo-text">Simulated session: nothing runs, nothing leaves this tab.</span>
  <a href="../">Website</a><a href="../docs/web-view/overview">Docs</a>
</div>`);
if (!html.includes('demo-mock.js')) throw new Error('sync: index.html changed shape, the demo shim was not injected');
writeFileSync(join(out, 'index.html'), html);

let js = readFileSync(join(from, 'app.js'), 'utf8');
// the app talks to absolute paths; the demo lives under /<base>/demo/
js = js.replaceAll("'/events'", "'events'").replaceAll("'/api/action'", "'api/action'")
  .replaceAll("'/favicon.svg'", "'favicon.svg'").replaceAll('`/api/log', '`api/log');
writeFileSync(join(out, 'app.js'), js);
copyFileSync(join(from, 'app.css'), join(out, 'app.css'));
console.log('sync: web view demo from src/web');

// the simulated session's catalog, in the shape of App::web_hello (plus the
// install fields the mock needs), with placeholders expanded like registry.rs
const appDir = (name) => `C:\\Users\\you\\scoop\\apps\\${name}\\current`;
const expand = (v, t) => (v ?? '').replaceAll('{scoop_app}', appDir(t.package)).replace(/\{app:([^}]+)\}/g, (_, n) => appDir(n));
const demoTools = fileOrder.map((t) => ({
  name: t.name,
  categories: t.categories,
  primary: t.primary,
  description: t.description,
  run_cmd: expand(t.run_cmd, t),
  args_hint: expand(t.args_hint, t),
  launch: t.launch,
  install: t.install,
  note: t.note,
  package: t.package,
  bucket: t.bucket,
  exe: expand(t.exe, t).split('\\').pop(),
  url: t.url ? expand(t.url, t) : null,
}));
const present = new Set(fileOrder.flatMap((t) => t.categories));
const tabs = [
  { key: 'all', label: 'All', blurb: 'all tools, grouped by main category' },
  { key: 'installed', label: 'Installed', blurb: 'tools installed on this machine' },
  ...categories.filter((c) => present.has(c.id)).map((c) => ({ key: `cat:${c.id}`, label: c.label, blurb: c.blurb })),
];
const hello = {
  version,
  catalog: 'C:\\Tools\\winsploit\\tools.yaml',
  slots,
  tools: demoTools,
  tabs,
  rank: fileOrder.map((t) => rankOf(t.primary)),
};
writeFileSync(join(out, 'demo-data.js'), `// generated by scripts/sync.mjs from tools.yaml\nwindow.__WS_DEMO__ = ${JSON.stringify({ slots, hello, tools: demoTools })};\n`);
for (const f of ['demo-mock.js', 'demo.css']) copyFileSync(join(site, 'demo-shim', f), join(out, f));
copyFileSync(join(site, 'public/favicon.svg'), join(out, 'favicon.svg'));
