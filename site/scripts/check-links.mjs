// After `npm run generate`: every internal href/src in the built HTML must
// resolve to a file in .output/public (and every #anchor to an id on that page).
import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../.output/public');
const base = (process.env.NUXT_APP_BASE_URL || '/').replace(/\/?$/, '/');

const pages = [];
(function walk(d) {
  for (const f of readdirSync(d)) {
    const p = join(d, f);
    if (statSync(p).isDirectory()) walk(p);
    else if (f.endsWith('.html')) pages.push(p);
  }
})(root);

const idsOf = new Map();
const ids = (file) => {
  if (!idsOf.has(file)) idsOf.set(file, new Set([...readFileSync(file, 'utf8').matchAll(/\sid="([^"]+)"/g)].map((m) => m[1])));
  return idsOf.get(file);
};
const fileFor = (path) => {
  const rel = decodeURIComponent(path.slice(base.length));
  for (const c of [rel, join(rel, 'index.html'), `${rel}.html`]) {
    const f = join(root, c);
    if (existsSync(f) && statSync(f).isFile()) return f;
  }
  return null;
};

let bad = 0;
for (const page of pages) {
  const html = readFileSync(page, 'utf8');
  for (const [, url] of html.matchAll(/(?:href|src)="([^"]+)"/g)) {
    if (!url.startsWith(base) || url.startsWith('//')) continue;
    const [path, hash] = url.split('#');
    const target = fileFor(path.split('?')[0]);
    const where = page.slice(root.length);
    if (!target) {
      console.error(`broken: ${url}  (in ${where})`);
      bad++;
    } else if (hash && target.endsWith('.html') && !ids(target).has(hash)) {
      console.error(`missing anchor: ${url}  (in ${where})`);
      bad++;
    }
  }
}
console.log(`check-links: ${pages.length} pages, ${bad} problems`);
process.exit(bad ? 1 : 0);
