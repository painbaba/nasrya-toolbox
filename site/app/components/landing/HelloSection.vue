<script setup lang="ts">
const cat = useCatalog();
const head = ref<HTMLElement>();
onMounted(() => {
  if (prefersReducedMotion() || !head.value) return;
  const { $gsap, $SplitText } = useNuxtApp();
  const split = $SplitText.create(head.value, { type: 'chars,lines', mask: 'lines', charsClass: 'split-char' });
  const tw = $gsap.from(split.chars, {
    yPercent: 120,
    rotate: 6,
    duration: 1.2,
    stagger: 0.025,
    scrollTrigger: { trigger: head.value, start: 'top 82%' },
  });
  onBeforeUnmount(() => {
    tw.scrollTrigger?.kill();
    tw.kill();
  });
});
</script>

<template>
  <section id="features" class="hello section">
    <div class="wrap hello__grid">
      <div class="hello__copy">
        <p class="eyebrow" v-reveal>Meet the dashboard</p>
        <h2 ref="head" class="h-lg hello__title">Hello, I'm <span class="accent">Winsploit</span>.</h2>
        <p class="lead" v-reveal="0.1">
          One terminal window for the whole toolbox. Browse the catalog by category, press <kbd>i</kbd> to install,
          <kbd>⏎</kbd> to run, and keep up to {{ cat.slots }} tools streaming side by side while you answer their
          prompts.
        </p>
        <ul class="hello__keys" v-reveal="0.2">
          <li><kbd>↑</kbd><kbd>↓</kbd> pick a tool</li>
          <li><kbd>←</kbd><kbd>→</kbd> switch category</li>
          <li><kbd>/</kbd> filter</li>
          <li><kbd>g</kbd> all terminals</li>
          <li><kbd>f</kbd> find in output</li>
          <li><kbd>?</kbd> every key</li>
        </ul>
        <NuxtLink to="/docs/using/keyboard" class="btn" v-reveal="0.3">Keyboard reference <ArrowIcon /></NuxtLink>
      </div>
      <div class="hello__demo" v-reveal="0.15">
        <TuiReplay />
      </div>
    </div>
  </section>
</template>

<style scoped>
.hello__grid { display: grid; grid-template-columns: minmax(0, 5fr) minmax(0, 7fr); gap: clamp(40px, 6vw, 96px); align-items: center; }
.hello__copy { display: grid; gap: 28px; justify-items: start; }
.hello__title { letter-spacing: -.05em; }
.hello__keys { list-style: none; padding: 0; margin: 0; display: grid; grid-template-columns: 1fr 1fr; gap: 10px 24px; font-family: var(--font-mono); font-size: 13px; color: var(--text-2); }
.hello__keys li { display: flex; align-items: center; gap: 6px; }
kbd {
  display: inline-grid; place-items: center; min-width: 24px; height: 24px; padding: 0 6px;
  font-family: var(--font-mono); font-size: 12px; color: var(--lg-100);
  background: var(--surface-2); border: 1px solid var(--line-strong); border-bottom-width: 2px; border-radius: 4px;
}
.lead kbd { height: 22px; min-width: 22px; font-size: 11px; vertical-align: 2px; }
@media (max-width: 1080px) { .hello__grid { grid-template-columns: 1fr; } }
</style>
