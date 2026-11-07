<script setup lang="ts">
// The browser view, shown live: the real web UI running against the demo's
// simulated session, framed in a browser window with a little parallax.
const demo = withBase('demo/');
const frame = ref<HTMLElement>();
const load = ref(false);
onMounted(() => {
  onVisible(frame.value!, () => (load.value = true), '200px 0px');
  if (prefersReducedMotion()) return;
  const { $gsap } = useNuxtApp();
  const tw = $gsap.fromTo(frame.value!, { yPercent: 8 }, {
    yPercent: -4, ease: 'none',
    scrollTrigger: { trigger: frame.value!, start: 'top bottom', end: 'bottom 30%', scrub: 0.8 },
  });
  onBeforeUnmount(() => { tw.scrollTrigger?.kill(); tw.kill(); });
});
const flags = [
  { f: 'w', d: 'start or stop it from the dashboard' },
  { f: '--web', d: 'start it and open the browser' },
  { f: '--port <n>', d: 'serve on a fixed port (default 7878)' },
  { f: 'W · --lan', d: 'serve your LAN too (plain HTTP: trusted networks only)' },
];
</script>

<template>
  <section id="web-view" class="wv section">
    <div class="wrap">
      <div class="wv__head">
        <div class="section-head">
          <p class="eyebrow" v-reveal>Web view</p>
          <h2 class="h-lg" v-reveal="0.05">The same session.<br /><span class="accent">In your browser.</span></h2>
        </div>
        <div class="wv__copy">
          <p class="lead" v-reveal="0.1">
            Press <kbd>w</kbd> and the dashboard is mirrored live in a browser tab: every terminal streaming, the
            catalog, installs, hash IDs. The terminal stays in charge. The browser sends the same actions your keys do,
            and never a command line.
          </p>
          <ul class="wv__flags mono" v-reveal="0.15">
            <li v-for="f in flags" :key="f.f"><code>{{ f.f }}</code><span>{{ f.d }}</span></li>
          </ul>
        </div>
      </div>

      <div class="wv__stage">
        <div ref="frame" class="window wv__browser">
          <div class="window__bar wv__bar">
            <span class="window__dots"><i /><i /><i /></span>
            <span class="wv__url"><span class="t-ok">●</span> 127.0.0.1:7878</span>
            <span class="wv__live mono">LIVE</span>
          </div>
          <div class="wv__viewport">
            <iframe v-if="load" :src="demo" title="Winsploit web view, simulated session" loading="lazy" tabindex="-1" />
            <a :href="demo" class="wv__overlay">
              <span class="btn btn--primary">Try the live demo <ArrowIcon /></span>
            </a>
          </div>
        </div>
      </div>
      <p class="wv__note mono" v-reveal>
        Off by default · 127.0.0.1 only unless you pass --lan · per-session token · Host and Origin checked ·
        <NuxtLink to="/docs/web-view/security" class="t-ok">security details →</NuxtLink>
      </p>
    </div>
  </section>
</template>

<style scoped>
.wv__head { display: grid; grid-template-columns: 1fr 1fr; gap: 48px; align-items: end; }
.wv__copy { display: grid; gap: 24px; }
.wv__flags { list-style: none; margin: 0; padding: 0; display: grid; gap: 0; font-size: 13px; }
.wv__flags li { display: grid; grid-template-columns: 120px 1fr; gap: 16px; padding: 10px 0; border-top: 1px solid var(--line); color: var(--text-2); }
.wv__flags code { color: var(--accent); }
.wv__stage { margin-top: 72px; perspective: 1400px; }
.wv__browser { transform-origin: 50% 100%; }
.wv__bar { justify-content: space-between; }
.wv__url { flex: 1; max-width: 360px; margin: 0 auto; padding: 3px 12px; border-radius: 999px; background: var(--dg-900); text-align: center; color: var(--text-2); }
.wv__live { font-size: 10px; letter-spacing: .14em; color: var(--accent); padding: 2px 6px; border: 1px solid var(--accent); border-radius: 2px; animation: live 2s steps(2) infinite; }
@keyframes live { 50% { opacity: .4; } }
.wv__viewport { position: relative; height: min(72vh, 720px); background: var(--dg-900); }
.wv__viewport iframe { width: 100%; height: 100%; border: 0; pointer-events: none; }
.wv__overlay { position: absolute; inset: 0; display: grid; place-items: center; background: linear-gradient(to top, color-mix(in srgb, var(--dg-900) 70%, transparent), transparent 50%); opacity: 0; transition: opacity .5s var(--ease-out); }
.wv__viewport:hover .wv__overlay, .wv__overlay:focus-visible { opacity: 1; }
.wv__note { margin-top: 24px; font-size: 12px; color: var(--text-3); }
kbd { font-family: var(--font-mono); font-size: 12px; padding: 1px 6px; border: 1px solid var(--line-strong); border-bottom-width: 2px; border-radius: 4px; color: var(--lg-100); }
@media (max-width: 980px) {
  .wv__head { grid-template-columns: 1fr; }
  .wv__viewport { height: 70vh; }
  .wv__overlay { opacity: 1; align-items: end; padding-bottom: 24px; }
}
</style>
