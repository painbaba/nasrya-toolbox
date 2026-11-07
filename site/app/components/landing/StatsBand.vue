<script setup lang="ts">
// Counters that tick up when they scroll in. Every number comes from the repo.
const cat = useCatalog();
const stats = [
  { n: cat.counts.tools!, label: 'curated tools', note: 'that run natively on Windows' },
  { n: cat.counts.categories!, label: 'categories', note: 'OSINT to wordlists' },
  { n: cat.slots, label: 'terminals at once', note: 'plus the always-open console' },
  { n: cat.hashFormats, label: 'hash formats', note: 'identified offline, with hashcat -m' },
  { n: 0, label: 'WSL, VMs or Docker', note: 'it is all native' },
];
const root = ref<HTMLElement>();
onMounted(() => {
  const els = [...root.value!.querySelectorAll<HTMLElement>('[data-n]')];
  if (prefersReducedMotion()) return;
  const { $gsap } = useNuxtApp();
  for (const el of els) el.textContent = '00';
  onVisible(root.value!, () => {
    els.forEach((el, i) => {
      const end = Number(el.dataset.n);
      const o = { v: 0 };
      $gsap.to(o, {
        v: end,
        duration: 1.8,
        delay: i * 0.12,
        ease: 'expo.out',
        onUpdate: () => (el.textContent = String(Math.round(o.v)).padStart(2, '0')),
      });
    });
  });
});
</script>

<template>
  <section ref="root" class="stats" aria-label="Winsploit in numbers">
    <div class="wrap stats__grid">
      <div v-for="(s, i) in stats" :key="s.label" class="stats__cell" v-reveal="i * 0.06">
        <span class="stats__idx mono">/{{ String(i + 1).padStart(2, '0') }}</span>
        <span class="stats__n" :data-n="s.n">{{ String(s.n).padStart(2, '0') }}</span>
        <span class="stats__label">{{ s.label }}</span>
        <span class="stats__note">{{ s.note }}</span>
      </div>
    </div>
  </section>
</template>

<style scoped>
.stats { border-block: 1px solid var(--line); }
.stats__grid { display: grid; grid-template-columns: repeat(5, 1fr); }
.stats__cell { position: relative; display: grid; gap: 6px; align-content: start; padding: 40px 24px 36px; border-left: 1px solid var(--line); }
.stats__cell:first-child { border-left: 0; padding-left: 0; }
.stats__idx { font-size: 11px; color: var(--text-3); letter-spacing: .1em; }
.stats__n { font-family: var(--font-pixel); font-size: clamp(56px, 6.4vw, 104px); line-height: 1; color: var(--accent); text-shadow: 0 0 40px var(--accent-glow); font-variant-numeric: tabular-nums; }
.stats__label { font-size: 18px; letter-spacing: -.01em; }
.stats__note { font-family: var(--font-mono); font-size: 12px; color: var(--text-3); }
@media (max-width: 1080px) {
  .stats__grid { grid-template-columns: repeat(2, 1fr); }
  .stats__cell { border-left: 0; border-top: 1px solid var(--line); padding-left: 0; }
  .stats__cell:nth-child(-n + 2) { border-top: 0; }
}
</style>
