<script setup lang="ts">
// The catalog as a live board: every tool in tools.yaml is a card. Category
// chips, launch-mode toggles and search reshuffle the board with GSAP Flip;
// clicking a card opens its detail panel (command, install route, caveats,
// the raw tools.yaml entry).
import type { Tool } from '~/composables/catalog';

const props = withDefaults(defineProps<{ compact?: boolean }>(), { compact: false });
const cat = useCatalog();
const route = useRoute();

const active = ref<string>((route.query.category as string) || 'all');
const mode = ref<string>('any');
const q = ref('');
const open = ref<Tool | null>(null);
const board = ref<HTMLElement>();

const MODES = [
  { id: 'any', label: 'Any' },
  { id: 'stream', label: 'Command line' },
  { id: 'gui', label: 'Desktop app' },
  { id: 'web', label: 'In browser' },
  { id: 'dataset', label: 'Data' },
];
const MODE_TAG: Record<string, string> = { stream: 'CLI', gui: 'GUI', web: 'WEB', dataset: 'DATA' };

const labelOf = (id: string) => cat.categories.find((c) => c.id === id)?.label ?? id;
const matches = (t: Tool) => {
  if (active.value !== 'all' && !t.categories.includes(active.value)) return false;
  if (mode.value !== 'any' && t.launch !== mode.value) return false;
  const words = q.value.toLowerCase().split(/\s+/).filter(Boolean);
  const hay = `${t.name} ${t.description} ${t.categories.join(' ')} ${t.install}`.toLowerCase();
  return words.every((w) => hay.includes(w));
};
const shown = computed(() => cat.tools.filter(matches));
const countFor = (id: string) => (id === 'all' ? cat.tools.length : cat.tools.filter((t) => t.categories.includes(id)).length);

// Flip: capture positions before the DOM changes, animate after
let flipState: any = null;
function capture() {
  if (!board.value || prefersReducedMotion()) return;
  const { $Flip } = useNuxtApp() as any;
  flipState = $Flip?.getState(board.value.querySelectorAll('.tcard'));
}
async function animate() {
  if (!flipState) return;
  await nextTick();
  const { $Flip } = useNuxtApp() as any;
  $Flip.from(flipState, {
    duration: 0.7,
    ease: 'expo.out',
    stagger: 0.012,
    absolute: true,
    onEnter: (els: Element[]) => useNuxtApp().$gsap.fromTo(els, { opacity: 0, scale: 0.85 }, { opacity: 1, scale: 1, duration: 0.6, stagger: 0.015 }),
    onLeave: (els: Element[]) => useNuxtApp().$gsap.to(els, { opacity: 0, scale: 0.85, duration: 0.35 }),
  });
  flipState = null;
}
function setCat(id: string) {
  if (id === active.value) return;
  capture();
  active.value = id;
  animate();
}
function setMode(id: string) {
  capture();
  mode.value = id;
  animate();
}
// detail panel
const yamlOf = (t: Tool) => {
  const lines = [`- name: ${t.name}`, `  categories: [${t.categories.join(', ')}]`, `  description: ${t.description}`];
  if (t.exe) lines.push(`  exe: ${t.exe.includes('\\') ? `'${t.exe}'` : t.exe}`);
  if (t.install === 'pipx') lines.push('  install: pipx', `  pipx: ${t.package}`);
  else lines.push(`  scoop: ${t.package}`);
  if (t.bucket && t.bucket !== 'main') lines.push(`  bucket: ${t.bucket}`);
  if (t.run_cmd) lines.push(`  run_cmd: ${t.run_cmd.includes('\\') ? `'${t.run_cmd}'` : t.run_cmd}`);
  if (t.args_hint) lines.push(`  args_hint: '${t.args_hint}'`);
  if (t.launch !== 'stream') lines.push(`  launch: ${t.launch}`);
  if (t.url) lines.push(`  url: '${t.url}'`);
  if (t.verify) lines.push(`  verify: { args: "${t.verify.args}", contains: "${t.verify.contains}" }`);
  if (t.note) lines.push(`  note: "${t.note}"`);
  return lines.join('\n');
};
const installCmd = (t: Tool) =>
  t.install === 'pipx' ? `pipx install ${t.package}` : t.bucket && t.bucket !== 'main' ? `scoop install ${t.bucket}/${t.package}` : `scoop install ${t.package}`;
const runCmd = (t: Tool) =>
  t.launch === 'dataset' ? '(dataset: Enter lists the files and copies the folder path)' : `${t.run_cmd || '(opens ' + (t.url ?? 'its page') + ')'} ${t.args_hint}`.trim();

defineExpose({ setCat });

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') open.value = null;
}
onMounted(() => window.addEventListener('keydown', onKey));
onBeforeUnmount(() => window.removeEventListener('keydown', onKey));

const copied = ref('');
async function copy(text: string, what: string) {
  try {
    await navigator.clipboard.writeText(text);
    copied.value = what;
    setTimeout(() => (copied.value = ''), 1400);
  } catch {
    /* clipboard blocked */
  }
}
</script>

<template>
  <div class="cx" :class="{ 'cx--compact': props.compact }">
    <div class="cx__controls">
      <div class="cx__cats" role="tablist" aria-label="Category">
        <button
          v-for="c in [{ id: 'all', label: 'All' }, ...cat.categories]"
          :key="c.id"
          role="tab"
          :aria-selected="active === c.id"
          class="cx__chip"
          :class="{ 'is-on': active === c.id }"
          @click="setCat(c.id)"
        >
          {{ c.label }}<sup>{{ countFor(c.id) }}</sup>
        </button>
      </div>
      <div class="cx__row">
        <label class="cx__search">
          <span class="t-ok mono">/</span>
          <input v-model="q" type="search" placeholder="Search tools, e.g. “subdomain” or “hash”" spellcheck="false" @input="capture(); animate()" />
        </label>
        <div class="cx__modes" role="radiogroup" aria-label="Launch mode">
          <button v-for="m in MODES" :key="m.id" role="radio" :aria-checked="mode === m.id" :class="{ 'is-on': mode === m.id }" @click="setMode(m.id)">{{ m.label }}</button>
        </div>
      </div>
      <p class="cx__blurb mono">
        <span class="t-ok">{{ String(shown.length).padStart(2, '0') }}</span> / {{ cat.tools.length }} tools
        <template v-if="active !== 'all'"> · {{ cat.categories.find((c) => c.id === active)?.blurb }}</template>
      </p>
    </div>

    <div ref="board" class="cx__board">
      <button
        v-for="t in shown"
        :key="t.name"
        :data-flip-id="t.name"
        class="tcard"
        :class="`tcard--${t.launch}`"
        @click="open = t"
      >
        <span class="tcard__top">
          <span class="tcard__cat mono">{{ labelOf(t.primary) }}</span>
          <span class="tcard__tag mono">{{ MODE_TAG[t.launch] }}</span>
        </span>
        <span class="tcard__name">{{ t.name }}</span>
        <span class="tcard__desc">{{ t.description }}</span>
        <span class="tcard__foot mono">
          <span>{{ t.install === 'pipx' ? 'pipx' : t.bucket === 'winsploit' ? 'winsploit bucket' : `scoop · ${t.bucket}` }}</span>
          <span v-if="t.verify" class="tcard__flag" title="Impostor check: verify:">verify</span>
          <span v-if="t.note" class="tcard__flag tcard__flag--warn" title="Has a caveat: note:">note</span>
        </span>
        <span class="tcard__px" aria-hidden="true" />
      </button>
      <p v-if="!shown.length" class="cx__empty mono">No tool matches. <button class="t-ok" @click="q = ''; mode = 'any'; active = 'all'">Reset filters</button></p>
    </div>

    <Teleport to="body">
      <Transition name="drawer">
        <div v-if="open" class="cx-drawer" role="dialog" aria-modal="true" :aria-label="open.name" @click.self="open = null">
          <aside class="cx-drawer__panel">
            <button class="cx-drawer__close mono" aria-label="Close" @click="open = null">ESC ✕</button>
            <p class="eyebrow">{{ open.categories.map(labelOf).join(' · ') }}</p>
            <h3 class="cx-drawer__name">{{ open.name }}</h3>
            <p class="cx-drawer__desc">{{ open.description }}</p>

            <div v-if="open.note" class="cx-drawer__note"><b>Note</b> {{ open.note }}</div>
            <div v-if="open.verify" class="cx-drawer__verify">
              <b>Impostor check</b> runs <code>{{ open.exe }} {{ open.verify.args }}</code> and expects
              <code>{{ open.verify.contains }}</code>. A same-named program on PATH shows as a conflict and is never launched.
            </div>

            <dl class="cx-drawer__facts mono">
              <dt>Launch</dt><dd>{{ MODES.find((m) => m.id === open!.launch)?.label }}</dd>
              <dt>Installs via</dt><dd>{{ open.install }}{{ open.bucket && open.install === 'scoop' ? ` (${open.bucket} bucket)` : '' }}</dd>
              <dt>Detected by</dt><dd>{{ open.exe || '—' }}</dd>
            </dl>

            <div class="cx-drawer__cmd">
              <div class="cx-drawer__cmdhead mono"><span>In the dashboard</span><span><kbd>i</kbd> install · <kbd>⏎</kbd> run</span></div>
              <pre><span class="t-dim"># what i runs</span>
<span class="t-ok">&gt;</span> {{ installCmd(open) }}
<span class="t-dim"># what ⏎ prefills</span>
<span class="t-ok">&gt;</span> {{ runCmd(open) }}</pre>
              <button class="cx-drawer__copy mono" @click="copy(runCmd(open!), 'run')">{{ copied === 'run' ? 'copied ✓' : 'copy command' }}</button>
            </div>

            <div class="cx-drawer__yaml">
              <div class="cx-drawer__cmdhead mono"><span>tools.yaml</span><button @click="copy(yamlOf(open!), 'yaml')">{{ copied === 'yaml' ? 'copied ✓' : 'copy' }}</button></div>
              <pre>{{ yamlOf(open) }}</pre>
            </div>
            <NuxtLink to="/docs/catalog/adding-a-tool" class="btn btn--sm" @click="open = null">Add a tool like this <ArrowIcon /></NuxtLink>
          </aside>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<style scoped>
.cx { display: grid; gap: 28px; }
.cx__controls { display: grid; gap: 16px; }
.cx__cats { display: flex; flex-wrap: wrap; gap: 6px; }
.cx__chip {
  height: 36px; padding: 0 14px; border: 1px solid var(--line); border-radius: 999px;
  font-family: var(--font-mono); font-size: 12.5px; letter-spacing: .04em; color: var(--text-2);
  transition: color .3s var(--ease-out), border-color .3s var(--ease-out), background-color .3s var(--ease-out);
}
.cx__chip sup { margin-left: 4px; font-size: 9.5px; color: var(--text-3); }
.cx__chip:hover { color: var(--text); border-color: var(--line-strong); }
.cx__chip.is-on { background: var(--accent); border-color: var(--accent); color: var(--accent-ink); }
.cx__chip.is-on sup { color: var(--dg-500); }
.cx__row { display: flex; flex-wrap: wrap; gap: 12px; }
.cx__search {
  flex: 1 1 280px; display: flex; align-items: center; gap: 10px; height: 44px; padding: 0 14px;
  border: 1px solid var(--line-strong); border-radius: var(--radius); background: var(--dg-800);
}
.cx__search:focus-within { border-color: var(--accent); }
.cx__search input { flex: 1; min-width: 0; background: none; border: 0; outline: none; color: var(--text); font: 15px var(--font-sans); }
.cx__modes { display: flex; border: 1px solid var(--line-strong); border-radius: var(--radius); overflow: hidden; flex-wrap: wrap; }
.cx__modes button { height: 42px; padding: 0 14px; font-family: var(--font-mono); font-size: 12px; color: var(--text-2); border-left: 1px solid var(--line); }
.cx__modes button:first-child { border-left: 0; }
.cx__modes button.is-on { background: var(--surface-3); color: var(--accent); }
.cx__blurb { font-size: 12.5px; color: var(--text-3); }

.cx__board { position: relative; display: grid; grid-template-columns: repeat(auto-fill, minmax(230px, 1fr)); gap: 10px; min-height: 200px; }
.tcard {
  position: relative; display: flex; flex-direction: column; gap: 8px; text-align: left; padding: 16px; min-height: 176px;
  background: var(--dg-800); border: 1px solid var(--line); border-radius: var(--radius); overflow: hidden;
  transition: border-color .4s var(--ease-out), background-color .4s var(--ease-out), transform .5s var(--ease-out);
}
.tcard:hover { border-color: color-mix(in srgb, var(--accent) 55%, transparent); background: var(--dg-700); transform: translateY(-3px); }
.tcard__top { display: flex; justify-content: space-between; font-size: 10.5px; letter-spacing: .1em; text-transform: uppercase; color: var(--text-3); }
.tcard__tag { padding: 1px 6px; border: 1px solid var(--line-strong); border-radius: 2px; }
.tcard--gui .tcard__tag { color: var(--amber); border-color: color-mix(in srgb, var(--amber) 40%, transparent); }
.tcard--web .tcard__tag { color: var(--syn-key); border-color: color-mix(in srgb, var(--syn-key) 40%, transparent); }
.tcard--dataset .tcard__tag { color: var(--syn-str); border-color: color-mix(in srgb, var(--syn-str) 40%, transparent); }
.tcard__name { font-size: 24px; letter-spacing: -.03em; color: var(--lg-50); line-height: 1.1; }
.tcard__desc { font-size: 13.5px; color: var(--text-2); line-height: 1.45; display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
.tcard__foot { margin-top: auto; display: flex; gap: 8px; align-items: center; font-size: 10.5px; color: var(--text-3); letter-spacing: .04em; }
.tcard__flag { padding: 0 5px; border-radius: 2px; background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--accent); }
.tcard__flag--warn { background: color-mix(in srgb, var(--warn) 14%, transparent); color: var(--warn); }
/* pixel corner that fills on hover */
.tcard__px { position: absolute; right: 0; bottom: 0; width: 36px; height: 36px; pointer-events: none;
  background-image: radial-gradient(var(--accent) 1.4px, transparent 1.6px); background-size: 6px 6px;
  opacity: 0; clip-path: polygon(100% 0, 100% 100%, 0 100%); transition: opacity .4s var(--ease-out); }
.tcard:hover .tcard__px { opacity: .7; }
.cx__empty { grid-column: 1 / -1; padding: 40px 0; color: var(--text-2); }

.cx--compact .cx__board { grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); }
.cx--compact .tcard { min-height: 150px; }
.cx--compact .tcard__desc { -webkit-line-clamp: 2; }

/* drawer */
.cx-drawer { position: fixed; inset: 0; z-index: 100; background: color-mix(in srgb, var(--dg-950) 70%, transparent); backdrop-filter: blur(4px); display: flex; justify-content: flex-end; }
.cx-drawer__panel {
  width: min(560px, 100%); height: 100%; overflow-y: auto; overscroll-behavior: contain; padding: 32px clamp(16px, 4vw, 40px) 40px;
  background: var(--dg-800); border-left: 1px solid var(--line-strong); display: flex; flex-direction: column; gap: 18px; align-items: flex-start;
}
.cx-drawer__close { align-self: flex-end; font-size: 11px; letter-spacing: .1em; color: var(--text-2); padding: 6px 10px; border: 1px solid var(--line); border-radius: 2px; }
.cx-drawer__close:hover { color: var(--text); border-color: var(--line-strong); }
.cx-drawer__name { font-size: clamp(40px, 6vw, 64px); letter-spacing: -.05em; color: var(--accent); }
.cx-drawer__desc { font-size: 18px; color: var(--lg-100); }
.cx-drawer__note, .cx-drawer__verify { width: 100%; padding: 12px 14px; border-left: 2px solid var(--warn); background: color-mix(in srgb, var(--warn) 7%, transparent); font-size: 14px; color: var(--lg-300); }
.cx-drawer__verify { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 6%, transparent); }
.cx-drawer__note b, .cx-drawer__verify b { display: block; font-family: var(--font-mono); font-size: 11px; text-transform: uppercase; letter-spacing: .1em; margin-bottom: 4px; color: var(--text); }
.cx-drawer code { font-size: 12.5px; color: var(--accent); }
.cx-drawer__facts { display: grid; grid-template-columns: auto 1fr; gap: 6px 20px; margin: 0; font-size: 13px; width: 100%; }
.cx-drawer__facts dt { color: var(--text-3); }
.cx-drawer__facts dd { margin: 0; color: var(--lg-100); word-break: break-all; }
.cx-drawer__cmd, .cx-drawer__yaml { width: 100%; border: 1px solid var(--line-strong); border-radius: var(--radius); background: var(--dg-900); overflow: hidden; position: relative; }
.cx-drawer__cmdhead { display: flex; justify-content: space-between; gap: 12px; padding: 8px 12px; border-bottom: 1px solid var(--line); font-size: 11px; letter-spacing: .06em; color: var(--text-3); }
.cx-drawer__cmdhead button { color: var(--accent); }
.cx-drawer pre { margin: 0; padding: 12px; font-size: 12.5px; line-height: 1.6; white-space: pre-wrap; word-break: break-all; color: var(--lg-300); }
.cx-drawer__copy { position: absolute; right: 10px; bottom: 10px; font-size: 11px; color: var(--accent); }
.cx-drawer kbd { font-size: 10px; padding: 0 4px; border: 1px solid var(--line-strong); border-radius: 3px; }
.drawer-enter-active, .drawer-leave-active { transition: opacity .45s var(--ease-out); }
.drawer-enter-active .cx-drawer__panel, .drawer-leave-active .cx-drawer__panel { transition: transform .6s var(--ease-out); }
.drawer-enter-from, .drawer-leave-to { opacity: 0; }
.drawer-enter-from .cx-drawer__panel, .drawer-leave-to .cx-drawer__panel { transform: translateX(100%); }
</style>
