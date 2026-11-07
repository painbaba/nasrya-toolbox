<script setup lang="ts">
const cat = useCatalog();
const cards = [
  {
    metric: 'SHA-256',
    title: 'Pinned downloads',
    text: 'Every manifest in Winsploit’s own Scoop bucket has a fixed URL and hash. Scoop refuses anything else.',
  },
  {
    metric: '0',
    title: 'Shell strings',
    text: 'Arguments are split quote-aware into a real argv. Programs resolve through PATH × PATHEXT, never the current directory.',
  },
  {
    metric: 'cwd ✕',
    title: 'No catalog from the folder you are in',
    text: 'A tools.yaml in a loot dir or cloned repo is ignored, and Winsploit tells you it skipped one. Opt in with --catalog.',
  },
  {
    metric: `${cat.counts.verified}+${cat.counts.noted}`,
    title: 'Impostor checks and caveats',
    text: 'verify: refuses the wrong same-named binary; note: shows each tool’s caveat (Npcap, admin, GPU drivers) on every run.',
  },
];
const scope = [
  { k: 'Works great', c: 'ok', items: 'nmap, nuclei, ffuf, gobuster, httpx, subfinder, katana, radare2, Ghidra' },
  { k: 'Works, with notes', c: 'warn', items: 'hashcat needs GPU drivers · tshark needs Npcap and admin · Python tools need pipx (bootstrapped)' },
  { k: 'Use Linux instead', c: 'err', items: 'Live Wi-Fi capture and injection (no monitor mode) · THC-Hydra, WPScan, WhatWeb, masscan' },
];
</script>

<template>
  <section class="sec section">
    <div class="wrap">
      <div class="section-head">
        <p class="eyebrow" v-reveal>Security model</p>
        <h2 class="h-lg" v-reveal="0.05">Built for folders full of <span class="accent">untrusted files.</span></h2>
        <p class="lead" v-reveal="0.1">It runs tools against real targets, often from CTF and loot directories. So it trusts very little.</p>
      </div>
      <div class="sec__grid">
        <article v-for="(c, i) in cards" :key="c.title" class="sec__card brackets" v-reveal="i * 0.08">
          <span class="sec__metric pixel">{{ c.metric }}</span>
          <h3>{{ c.title }}</h3>
          <p>{{ c.text }}</p>
        </article>
      </div>

      <div class="scope" v-reveal>
        <p class="eyebrow">Honest scope</p>
        <div class="scope__grid">
          <div v-for="s in scope" :key="s.k" class="scope__col" :class="`is-${s.c}`">
            <h4 class="mono">{{ s.k }}</h4>
            <p>{{ s.items }}</p>
          </div>
        </div>
      </div>
      <NuxtLink to="/docs/security/model" class="btn sec__btn" v-reveal>Read the security model <ArrowIcon /></NuxtLink>
    </div>
  </section>
</template>

<style scoped>
.sec__grid { margin-top: 72px; display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px; }
.sec__card { display: flex; flex-direction: column; gap: 14px; padding: 28px 24px; min-height: 300px; border: 1px solid var(--line); border-radius: var(--radius); background: linear-gradient(to bottom, var(--dg-800), var(--dg-900)); transition: border-color .5s var(--ease-out); }
.sec__card:hover { border-color: var(--line-strong); }
.sec__metric { font-size: clamp(40px, 4vw, 58px); line-height: 1; color: var(--accent); margin-bottom: auto; padding-bottom: 32px; }
.sec__card h3 { font-size: 21px; letter-spacing: -.02em; line-height: 1.2; }
.sec__card p { color: var(--text-2); font-size: 14.5px; }
.scope { margin-top: 96px; display: grid; gap: 24px; }
.scope__grid { display: grid; grid-template-columns: repeat(3, 1fr); border: 1px solid var(--line); border-radius: var(--radius); overflow: hidden; }
.scope__col { padding: 24px; border-left: 1px solid var(--line); display: grid; gap: 10px; align-content: start; }
.scope__col:first-child { border-left: 0; }
.scope__col h4 { font-size: 12px; letter-spacing: .12em; text-transform: uppercase; display: flex; align-items: center; gap: 10px; }
.scope__col h4::before { content: ''; width: 8px; height: 8px; background: currentColor; box-shadow: 0 0 12px currentColor; }
.scope__col p { color: var(--lg-300); font-size: 15px; }
.is-ok h4 { color: var(--accent); } .is-warn h4 { color: var(--warn); } .is-err h4 { color: var(--risk); }
.sec__btn { margin-top: 40px; }
@media (max-width: 1080px) { .sec__grid { grid-template-columns: 1fr 1fr; } }
@media (max-width: 720px) {
  .sec__grid, .scope__grid { grid-template-columns: 1fr; }
  .scope__col { border-left: 0; border-top: 1px solid var(--line); }
  .scope__col:first-child { border-top: 0; }
  .sec__card { min-height: 0; }
}
</style>
