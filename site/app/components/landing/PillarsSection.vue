<script setup lang="ts">
const cat = useCatalog();
const pillars = [
  {
    n: '01',
    title: 'Find',
    sub: 'Know what is on this machine.',
    text: 'Detection probes PATH × PATHEXT off the UI thread, so the dashboard never freezes, and it checks that the program it found is the right one.',
    bullets: [
      `Browse ${cat.counts.categories} categories with live install status`,
      'Filter by name, category or description with /',
      'Same-named impostors show as a conflict and never launch',
      'PATH changes are picked up live, no restart',
    ],
    to: '/docs/using/dashboard',
  },
  {
    n: '02',
    title: 'Install',
    sub: 'One key. Done properly.',
    text: 'Scoop for native tools, pipx for Python tools, and Winsploit’s own hash-pinned bucket for the ones nobody packages.',
    bullets: [
      'Missing Scoop buckets are added on first use',
      'Each Python tool gets its own venv; Python and pipx are bootstrapped',
      'Every manifest in the Winsploit bucket has a pinned URL and SHA-256',
      'Every install leaves a timestamped log',
    ],
    to: '/docs/using/installing',
  },
  {
    n: '03',
    title: 'Run',
    sub: 'Live, interactive, side by side.',
    text: `Up to ${cat.slots} tools stream at once, each in its own terminal with its own input pipe, scrollback and search.`,
    bullets: [
      'Answer prompts without fighting the dashboard for keys',
      'Find in output with f · n · N; g shows every terminal',
      'c stops the whole process tree, not just the parent',
      'Progress bars and ANSI codes are cleaned, not smeared',
    ],
    to: '/docs/using/running-tools',
  },
];
const glyphs = ['●', '●', '○', '●', '◐', '●', '●', '○', '●', '●', '●', '○', '●', '●', '●', '●', '○', '●', '●', '●', '●', '○', '●', '●'];
</script>

<template>
  <section class="pillars section">
    <div class="wrap">
      <div class="section-head">
        <p class="eyebrow" v-reveal>How it works</p>
        <h2 class="h-lg" v-reveal="0.05">Find it. Install it. <span class="accent">Run it.</span></h2>
      </div>
      <div class="pillars__grid">
        <article v-for="(p, i) in pillars" :key="p.n" class="pillar brackets" v-reveal="i * 0.1">
          <div class="pillar__visual" :class="`v-${p.title.toLowerCase()}`" aria-hidden="true">
            <template v-if="i === 0">
              <span v-for="(g, k) in glyphs" :key="k" :class="g === '◐' ? 'risk' : g === '○' ? 'dim' : ''" :style="{ '--k': k }">{{ g }}</span>
              <i class="scan" />
            </template>
            <template v-else-if="i === 1">
              <div class="bar"><span v-for="k in 20" :key="k" :style="{ '--k': k }" /></div>
              <div class="bar-meta"><span>scoop install security/ffuf</span><b>sha256 ✓</b></div>
            </template>
            <template v-else>
              <div v-for="k in 4" :key="k" class="mini"><i v-for="l in 5" :key="l" :style="{ '--l': l, '--k': k }" /></div>
            </template>
          </div>
          <div class="pillar__head">
            <span class="pillar__n mono">{{ p.n }}</span>
            <h3 class="h-md">{{ p.title }}</h3>
          </div>
          <p class="pillar__sub">{{ p.sub }}</p>
          <p class="pillar__text">{{ p.text }}</p>
          <ul class="pillar__list">
            <li v-for="b in p.bullets" :key="b">{{ b }}</li>
          </ul>
          <NuxtLink :to="p.to" class="pillar__link">Read the docs <ArrowIcon /></NuxtLink>
        </article>
      </div>
    </div>
  </section>
</template>

<style scoped>
.pillars__grid { margin-top: 72px; display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; }
.pillar { display: flex; flex-direction: column; gap: 16px; padding: 24px; background: var(--dg-800); border: 1px solid var(--line); border-radius: var(--radius); transition: border-color .5s var(--ease-out), background-color .5s var(--ease-out); }
.pillar:hover { border-color: var(--line-strong); background: var(--dg-700); }
.pillar__visual { height: 168px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--dg-900); position: relative; overflow: hidden; margin-bottom: 8px; }
.pillar__head { display: flex; align-items: baseline; gap: 14px; }
.pillar__n { font-size: 12px; color: var(--accent); }
.pillar__sub { font-size: 18px; color: var(--lg-100); }
.pillar__text { color: var(--text-2); }
.pillar__list { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; font-size: 14px; color: var(--lg-300); }
.pillar__list li { display: flex; gap: 10px; }
.pillar__list li::before { content: '▸'; color: var(--accent); font-family: var(--font-mono); }
.pillar__link { margin-top: auto; padding-top: 12px; display: inline-flex; align-items: center; gap: 8px; font-family: var(--font-mono); font-size: 12px; letter-spacing: .1em; text-transform: uppercase; color: var(--accent); }
.pillar__link :deep(svg) { width: 14px; transition: transform .5s var(--ease-out); }
.pillar__link:hover :deep(svg) { transform: translateX(4px); }

/* find: status glyphs resolving under a scan line */
.v-find { display: grid; grid-template-columns: repeat(8, 1fr); align-content: center; padding: 18px; gap: 10px 0; font-family: var(--font-mono); font-size: 18px; text-align: center; color: var(--accent); }
.v-find span { animation: resolve 4s steps(1) infinite; animation-delay: calc(var(--k) * 70ms); }
.v-find .dim { color: var(--text-3); }
.v-find .risk { color: var(--risk); }
@keyframes resolve { 0% { opacity: .15; } 30%, 100% { opacity: 1; } }
.v-find .scan { position: absolute; top: 0; bottom: 0; width: 2px; background: var(--accent); box-shadow: 0 0 18px 4px var(--accent-glow); animation: scan 4s var(--ease-io) infinite; }
@keyframes scan { 0% { left: -2%; } 45%, 100% { left: 102%; } }

/* install: a pixel progress bar */
.v-install { display: grid; align-content: center; gap: 14px; padding: 24px; }
.bar { display: grid; grid-template-columns: repeat(20, 1fr); gap: 3px; }
.bar span { height: 22px; background: var(--dg-500); animation: fill 3.6s steps(1) infinite; animation-delay: calc(var(--k) * 90ms); }
@keyframes fill { 0% { background: var(--dg-500); } 8%, 82% { background: var(--accent); box-shadow: 0 0 10px var(--accent-glow); } 83%, 100% { background: var(--dg-500); } }
.bar-meta { display: flex; justify-content: space-between; font-family: var(--font-mono); font-size: 11.5px; color: var(--text-3); gap: 12px; }
.bar-meta b { font-weight: 500; color: var(--accent); animation: ok 3.6s steps(1) infinite; white-space: nowrap; }
@keyframes ok { 0%, 55% { opacity: 0; } 56%, 82% { opacity: 1; } 83% { opacity: 0; } }

/* run: four tiny terminals streaming */
.v-run { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; padding: 12px; }
.mini { border: 1px solid var(--line-strong); border-radius: 3px; padding: 8px; display: grid; gap: 5px; align-content: end; }
.mini i { display: block; height: 4px; border-radius: 1px; background: var(--lg-700); transform-origin: left; animation: line 2.4s var(--ease-out) infinite; animation-delay: calc(var(--l) * 160ms + var(--k) * 270ms); width: calc(30% + var(--l) * 11%); }
.mini i:first-child { background: var(--accent); width: 60%; }
@keyframes line { 0% { transform: scaleX(0); } 40%, 100% { transform: scaleX(1); } }

@media (max-width: 1080px) { .pillars__grid { grid-template-columns: 1fr; } }
</style>
