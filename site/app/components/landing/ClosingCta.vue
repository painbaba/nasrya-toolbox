<script setup lang="ts">
const demo = withBase('demo/');
</script>

<template>
  <section class="cta">
    <HeroCanvas :intensity="0.75" :cell="11" />
    <div class="cta__shade" />
    <div class="wrap cta__inner">
      <p class="eyebrow" v-reveal>Get Winsploit</p>
      <h2 class="h-xl cta__title" v-reveal="0.05">Stop fighting<br />your toolchain.<br /><span class="accent">Start testing.</span></h2>
      <div class="cta__btns" v-reveal="0.15">
        <NuxtLink to="/docs/get-started/quickstart" class="btn btn--primary">Quick start <ArrowIcon /></NuxtLink>
        <a :href="demo" class="btn">Try the live demo <ArrowIcon /></a>
      </div>
      <p class="cta__legal mono" v-reveal="0.2">
        ⚠ Authorized use only. Point it only at systems you own or have explicit written permission to test.
      </p>
    </div>
  </section>
</template>

<style scoped>
.cta { position: relative; overflow: hidden; padding: clamp(120px, 18vw, 240px) 0; border-top: 1px solid var(--line); }
.cta__shade { position: absolute; inset: 0; pointer-events: none; background: radial-gradient(ellipse 60% 70% at 30% 55%, color-mix(in srgb, var(--dg-900) 88%, transparent), transparent 75%), linear-gradient(to bottom, var(--dg-900), transparent 25%); }
.cta__inner { position: relative; z-index: 1; display: grid; gap: 32px; justify-items: start; pointer-events: none; }
.cta__inner > * { pointer-events: auto; }
.cta__title { letter-spacing: -.055em; line-height: .92; }
.cta__btns { display: flex; flex-wrap: wrap; gap: 12px; }
.cta__legal { font-size: 12px; color: var(--text-3); max-width: 560px; }
</style>
