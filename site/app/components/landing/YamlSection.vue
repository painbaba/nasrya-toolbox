<script setup lang="ts">
// "Add a tool in a few lines": a code window types a real tools.yaml entry,
// then a console window confirms the catalog loaded.
const cat = useCatalog();
const g = cat.tools.find((t) => t.name === 'gobuster');
// [class, text] tokens, one entry per line
const LINES: [string, string][][] = [
  [['t-dim', '  '], ['t-fn', '- '], ['t-key', 'name'], ['', ': '], ['t-str', 'gobuster']],
  [['t-dim', '    '], ['t-key', 'categories'], ['', ': ['], ['t-str', (g?.categories ?? ['recon', 'web']).join(', ')], ['', ']']],
  [['t-dim', '    '], ['t-key', 'description'], ['', ': '], ['t-str', g?.description ?? 'Directory / DNS / vhost brute-forcing']],
  [['t-dim', '    '], ['t-key', 'exe'], ['', ': '], ['t-str', 'gobuster'], ['t-com', '        # detect it on PATH']],
  [['t-dim', '    '], ['t-key', 'scoop'], ['', ': '], ['t-str', 'gobuster'], ['t-com', '      # install it']],
  [['t-dim', '    '], ['t-key', 'run_cmd'], ['', ': '], ['t-str', 'gobuster'], ['t-com', '    # what Enter runs']],
  [['t-dim', '    '], ['t-key', 'args_hint'], ['', ': '], ['t-str', `'${g?.args_hint ?? 'dir -u https://example.com'}'`]],
];
const total = LINES.flat().reduce((n, [, s]) => n + s.length, 0);
const typed = ref(total);
const done = ref(true);
const root = ref<HTMLElement>();

const view = computed(() => {
  let left = typed.value;
  return LINES.map((line) =>
    line.map(([c, s]) => {
      const take = Math.max(0, Math.min(s.length, left));
      left -= s.length;
      return [c, s.slice(0, take)] as [string, string];
    }),
  );
});

onMounted(() => {
  if (prefersReducedMotion()) return;
  typed.value = 0;
  done.value = false;
  onVisible(root.value!, () => {
    const { $gsap } = useNuxtApp();
    const o = { v: 0 };
    $gsap.to(o, {
      v: total,
      duration: total / 55,
      ease: 'none',
      delay: 0.3,
      onUpdate: () => (typed.value = Math.round(o.v)),
      onComplete: () => setTimeout(() => (done.value = true), 400),
    });
  });
});
</script>

<template>
  <section ref="root" class="yaml section">
    <div class="wrap yaml__grid">
      <div class="yaml__copy">
        <p class="eyebrow" v-reveal>Extend it</p>
        <h2 class="h-lg" v-reveal="0.05">Add a tool in <span class="accent">seven lines.</span></h2>
        <p class="lead" v-reveal="0.1">
          The catalog is the extension point. No code: launch modes, placeholders, impostor checks and caveats are
          all YAML, and every problem in the file is reported at once.
        </p>
        <ul class="yaml__list mono" v-reveal="0.15">
          <li><span class="t-ok">launch:</span> stream · gui · web · dataset</li>
          <li><span class="t-ok">install:</span> scoop (default) · pipx</li>
          <li><span class="t-ok">{scoop_app}</span> · <span class="t-ok">{app:NAME}</span> placeholders</li>
          <li><span class="t-ok">verify:</span> · <span class="t-ok">note:</span> impostors and caveats</li>
        </ul>
        <NuxtLink to="/docs/catalog/tools-yaml" class="btn" v-reveal="0.2">tools.yaml reference <ArrowIcon /></NuxtLink>
      </div>
      <div class="yaml__stack" v-reveal="0.1">
        <div class="window">
          <div class="window__bar"><span class="window__dots"><i /><i /><i /></span><span>tools.yaml</span></div>
          <pre class="yaml__code"><code><span v-for="(line, i) in view" :key="i" class="yaml__line"><span class="yaml__ln">{{ 60 + i }}</span><span v-for="(tok, k) in line" :key="k" :class="tok[0]">{{ tok[1] }}</span><i v-if="!done && (i === view.findLastIndex((l) => l.some((t) => t[1])) )" class="yaml__caret" />
</span></code></pre>
        </div>
        <div class="window yaml__console" :class="{ 'is-on': done }">
          <div class="window__bar"><span class="window__dots"><i /><i /><i /></span><span>winsploit list</span></div>
          <pre class="yaml__code"><code><span class="t-ok">✓</span> catalog loaded · {{ cat.counts.tools }} tools · 0 problems
<span class="t-ok">●</span> gobuster          recon, web     installed</code></pre>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.yaml__grid { display: grid; grid-template-columns: 1fr 1.15fr; gap: clamp(40px, 6vw, 96px); align-items: center; }
.yaml__copy { display: grid; gap: 28px; justify-items: start; }
.yaml__list { list-style: none; margin: 0; padding: 0; display: grid; gap: 10px; font-size: 13.5px; color: var(--text-2); }
.yaml__stack { display: grid; gap: 16px; min-width: 0; }
.yaml__code { margin: 0; padding: 18px 0; font-size: 13.5px; line-height: 1.75; overflow-x: auto; color: var(--lg-100); }
.yaml__line { display: block; padding-right: 18px; white-space: pre; }
.yaml__ln { display: inline-block; width: 44px; padding-right: 14px; text-align: right; color: var(--dg-400); user-select: none; }
.yaml__caret { display: inline-block; width: 8px; height: 16px; background: var(--accent); vertical-align: -3px; animation: blink 1s steps(2) infinite; }
.yaml__console { opacity: .25; transform: translateY(12px); transition: opacity .8s var(--ease-out), transform .8s var(--ease-out); }
.yaml__console .yaml__code { padding: 16px 18px; }
.yaml__console.is-on { opacity: 1; transform: none; }
@keyframes blink { 50% { opacity: 0; } }
@media (max-width: 1080px) { .yaml__grid { grid-template-columns: 1fr; } }
</style>
