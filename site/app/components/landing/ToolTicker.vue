<script setup lang="ts">
// Two marquee rows of catalog tools with status pills, like the dashboard's
// list. Scrolling speeds them up and flips their direction.
const cat = useCatalog();
type St = 'installed' | 'running' | 'conflict' | 'missing';
const LABEL: Record<St, string> = { installed: '● installed', running: '▸ running', conflict: '◐ conflict!', missing: '○ missing' };

function statusOf(name: string, i: number): St {
  if (name === 'httpx') return 'conflict'; // the real example: ProjectDiscovery vs Python httpx
  if (i % 7 === 2) return 'running';
  if (i % 5 === 4) return 'missing';
  return 'installed';
}
const all = cat.tools.map((t, i) => ({ name: t.name, st: statusOf(t.name, i) }));
const half = Math.ceil(all.length / 2);
const rows = [all.slice(0, half), all.slice(half)];

const root = ref<HTMLElement>();
onMounted(() => {
  if (prefersReducedMotion() || !root.value) return;
  const { $gsap, $ScrollTrigger } = useNuxtApp();
  const tracks = [...root.value.querySelectorAll<HTMLElement>('.ticker__track')];
  const tweens = tracks.map((tr, i) =>
    $gsap.fromTo(tr, { xPercent: i ? -50 : 0 }, { xPercent: i ? 0 : -50, duration: 60 + i * 12, ease: 'none', repeat: -1 }),
  );
  let dir = 1;
  let settle: ReturnType<typeof $gsap.delayedCall> | null = null;
  const st = $ScrollTrigger.create({
    trigger: root.value,
    start: 'top bottom',
    end: 'bottom top',
    onUpdate(self) {
      const v = self.getVelocity();
      if (Math.abs(v) > 10) dir = v > 0 ? 1 : -1;
      const boost = 1 + Math.min(6, Math.abs(v) / 300);
      for (const t of tweens) $gsap.to(t, { timeScale: dir * boost, duration: 0.3, overwrite: true });
      settle?.kill();
      settle = $gsap.delayedCall(0.4, () => tweens.forEach((t) => $gsap.to(t, { timeScale: dir, duration: 1.2, overwrite: true })));
    },
  });
  onBeforeUnmount(() => {
    tweens.forEach((t) => t.kill());
    st.kill();
  });
});
</script>

<template>
  <section ref="root" class="ticker" aria-label="Some of the tools in the catalog">
    <div v-for="(row, r) in rows" :key="r" class="ticker__row">
      <div class="ticker__track">
        <template v-for="copy in 2" :key="copy">
          <span v-for="t in row" :key="t.name + copy" class="ticker__item" :aria-hidden="copy === 2">
            <span class="ticker__name">{{ t.name }}</span>
            <span class="ticker__pill" :class="`is-${t.st}`">{{ LABEL[t.st] }}</span>
          </span>
        </template>
      </div>
    </div>
  </section>
</template>

<style scoped>
.ticker { border-block: 1px solid var(--line); padding: 18px 0; overflow: hidden; background: var(--dg-900); position: relative; }
.ticker::before, .ticker::after { content: ''; position: absolute; top: 0; bottom: 0; width: 12vw; z-index: 1; pointer-events: none; }
.ticker::before { left: 0; background: linear-gradient(90deg, var(--dg-900), transparent); }
.ticker::after { right: 0; background: linear-gradient(-90deg, var(--dg-900), transparent); }
.ticker__row { overflow: hidden; }
.ticker__row + .ticker__row { margin-top: 12px; }
.ticker__track { display: flex; width: max-content; will-change: transform; }
.ticker__item { display: inline-flex; align-items: center; gap: 12px; padding: 0 28px; border-right: 1px solid var(--line); white-space: nowrap; }
.ticker__name { font-size: clamp(22px, 2.6vw, 34px); letter-spacing: -.03em; color: var(--lg-100); }
.ticker__pill { font-family: var(--font-mono); font-size: 11px; letter-spacing: .08em; text-transform: uppercase; padding: 3px 8px; border-radius: 2px; border: 1px solid currentColor; }
.is-installed { color: var(--accent); }
.is-running { color: var(--bg-100); background: color-mix(in srgb, var(--accent) 16%, transparent); animation: pulse 1.6s steps(2) infinite; }
.is-conflict { color: var(--risk); background: color-mix(in srgb, var(--risk) 14%, transparent); }
.is-missing { color: var(--text-3); }
@keyframes pulse { 50% { border-color: transparent; } }
</style>
