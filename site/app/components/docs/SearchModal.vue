<script setup lang="ts">
// Ctrl/⌘ K search over every docs section. The index is search.json,
// prerendered from Nuxt Content's search sections; MiniSearch runs in the tab.
import MiniSearch from 'minisearch';

interface Section { id: string; title: string; titles: string[]; content: string; level: number }
const open = useState('docs-search', () => false);
const q = ref('');
const active = ref(0);
const input = ref<HTMLInputElement>();
let ms: MiniSearch<Section> | null = null;
const byId = new Map<string, Section>();
const results = ref<(Section & { score: number })[]>([]);

async function ensureIndex() {
  if (ms) return;
  const data: Section[] = await $fetch(withBase('search.json'));
  ms = new MiniSearch<Section>({
    fields: ['title', 'titles', 'content'],
    storeFields: ['id'],
    extractField: (doc, f) => (f === 'titles' ? doc.titles.join(' ') : (doc as any)[f]),
    searchOptions: { prefix: true, fuzzy: 0.2, boost: { title: 4, titles: 2 } },
  });
  for (const d of data) byId.set(d.id, d);
  ms.addAll(data);
}

watch(open, async (v) => {
  if (!v) return;
  q.value = '';
  results.value = [];
  await nextTick();
  input.value?.focus();
  ensureIndex();
});
watch(q, async (v) => {
  await ensureIndex();
  results.value = v.trim() ? ms!.search(v).slice(0, 12).map((r) => ({ ...byId.get(r.id)!, score: r.score })) : [];
  active.value = 0;
});

function snippet(text: string) {
  const words = q.value.toLowerCase().split(/\s+/).filter(Boolean);
  const lower = text.toLowerCase();
  const at = Math.max(0, ...words.map((w) => lower.indexOf(w)));
  const start = Math.max(0, at - 50);
  return (start ? '…' : '') + text.slice(start, start + 140) + (text.length > start + 140 ? '…' : '');
}

const router = useRouter();
function go(r: Section) {
  open.value = false;
  router.push(r.id);
}
function onKey(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    open.value = !open.value;
    return;
  }
  if (!open.value) {
    if (e.key === '/' && !(e.target as HTMLElement)?.closest('input,textarea')) {
      e.preventDefault();
      open.value = true;
    }
    return;
  }
  if (e.key === 'Escape') open.value = false;
  else if (e.key === 'ArrowDown') { e.preventDefault(); active.value = Math.min(active.value + 1, results.value.length - 1); }
  else if (e.key === 'ArrowUp') { e.preventDefault(); active.value = Math.max(active.value - 1, 0); }
  else if (e.key === 'Enter' && results.value[active.value]) go(results.value[active.value]!);
}
onMounted(() => window.addEventListener('keydown', onKey));
onBeforeUnmount(() => window.removeEventListener('keydown', onKey));
</script>

<template>
  <Teleport to="body">
    <Transition name="sm">
      <div v-if="open" class="sm" @click.self="open = false">
        <div class="sm__box" role="dialog" aria-modal="true" aria-label="Search the docs">
          <label class="sm__field">
            <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" /></svg>
            <input ref="input" v-model="q" placeholder="Search the docs: “verify”, “--lan”, “hashcat”…" spellcheck="false" />
            <kbd>Esc</kbd>
          </label>
          <ul v-if="results.length" class="sm__list">
            <li v-for="(r, i) in results" :key="r.id">
              <button :class="{ 'is-on': i === active }" @mouseenter="active = i" @click="go(r)">
                <span class="sm__path mono">{{ [...r.titles, r.title].join(' › ') }}</span>
                <span class="sm__snip">{{ snippet(r.content) }}</span>
              </button>
            </li>
          </ul>
          <p v-else-if="q.trim()" class="sm__empty mono">No results for “{{ q }}”.</p>
          <div v-else class="sm__hints">
            <p class="mono">Try</p>
            <button v-for="s in ['quick start', 'verify', '--lan', 'placeholders', 'hash', 'logs']" :key="s" class="sm__chip mono" @click="q = s">{{ s }}</button>
          </div>
          <footer class="sm__foot mono"><span><kbd>↑</kbd><kbd>↓</kbd> move</span><span><kbd>⏎</kbd> open</span><span><kbd>/</kbd> or <kbd>Ctrl K</kbd> anywhere</span></footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.sm { position: fixed; inset: 0; z-index: 200; background: color-mix(in srgb, var(--dg-950) 75%, transparent); backdrop-filter: blur(3px); display: flex; justify-content: center; align-items: flex-start; padding: 12vh 16px 16px; }
.sm__box { width: min(680px, 100%); max-height: 72vh; display: flex; flex-direction: column; background: var(--dg-800); border: 1px solid var(--line-strong); border-radius: 12px; box-shadow: 0 40px 120px rgba(0, 0, 0, .7); overflow: hidden; }
.sm__field { display: flex; align-items: center; gap: 12px; padding: 0 16px; height: 56px; border-bottom: 1px solid var(--line); }
.sm__field svg { width: 18px; height: 18px; fill: none; stroke: var(--accent); stroke-width: 2; flex: none; }
.sm__field input { flex: 1; min-width: 0; background: none; border: 0; outline: none; color: var(--text); font: 17px var(--font-sans); }
.sm__list { list-style: none; margin: 0; padding: 8px; overflow-y: auto; }
.sm__list button { width: 100%; text-align: left; display: grid; gap: 4px; padding: 10px 12px; border-radius: 8px; }
.sm__list button.is-on { background: var(--surface-3); box-shadow: inset 2px 0 0 var(--accent); }
.sm__path { font-size: 12px; color: var(--accent); }
.sm__snip { font-size: 14px; color: var(--text-2); line-height: 1.45; }
.sm__empty { padding: 28px 18px; color: var(--text-3); font-size: 13px; }
.sm__hints { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 18px; }
.sm__hints p { font-size: 12px; color: var(--text-3); margin-right: 4px; }
.sm__chip { font-size: 12px; padding: 4px 10px; border: 1px solid var(--line-strong); border-radius: 999px; color: var(--text-2); }
.sm__chip:hover { color: var(--accent); border-color: var(--accent); }
.sm__foot { display: flex; gap: 18px; padding: 10px 16px; border-top: 1px solid var(--line); font-size: 11.5px; color: var(--text-3); }
.sm__foot kbd { height: 18px; min-width: 18px; font-size: 10px; margin-right: 3px; }
.sm-enter-active, .sm-leave-active { transition: opacity .25s var(--ease-out); }
.sm-enter-active .sm__box, .sm-leave-active .sm__box { transition: transform .35s var(--ease-out); }
.sm-enter-from, .sm-leave-to { opacity: 0; }
.sm-enter-from .sm__box { transform: translateY(-10px) scale(.98); }
@media (max-width: 640px) { .sm__foot { display: none; } .sm { padding-top: 70px; } }
</style>
