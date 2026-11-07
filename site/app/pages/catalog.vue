<script setup lang="ts">
const cat = useCatalog();
useLenis();
useSeoMeta({
  title: `Catalog · ${cat.counts.tools} tools · Winsploit`,
  description: `Every tool Winsploit can install and run natively on Windows: ${cat.categories.map((c) => c.label).join(', ')}.`,
});
const explorer = ref<{ setCat: (id: string) => void }>();
const counts = [
  { n: cat.counts.tools, l: 'tools' },
  { n: cat.counts.scoop, l: 'via Scoop' },
  { n: cat.counts.pipx, l: 'via pipx' },
  { n: cat.counts.ownBucket, l: 'from Winsploit’s bucket' },
  { n: cat.counts.gui, l: 'desktop apps' },
];
</script>

<template>
  <main class="catpage">
    <section class="catpage__hero">
      <HeroCanvas :intensity="0.6" :cell="10" />
      <div class="catpage__shade" />
      <div class="wrap catpage__head">
        <p class="eyebrow">The catalog · generated from tools.yaml</p>
        <h1 class="h-xl">Everything you<br /><span class="accent">can run.</span></h1>
        <ul class="catpage__counts mono">
          <li v-for="c in counts" :key="c.l"><b class="pixel">{{ c.n }}</b> {{ c.l }}</li>
        </ul>
      </div>
    </section>
    <section class="wrap catpage__matrix">
      <CategoryMatrix @pick="(id) => explorer?.setCat(id)" />
    </section>
    <section class="wrap catpage__board">
      <CatalogExplorer ref="explorer" />
    </section>
    <section class="wrap catpage__foot">
      <p class="lead">Missing something? Every tool is a YAML entry: no code needed.</p>
      <NuxtLink to="/docs/catalog/adding-a-tool" class="btn btn--primary">Add a tool <ArrowIcon /></NuxtLink>
    </section>
  </main>
</template>

<style scoped>
.catpage__hero { position: relative; overflow: hidden; padding: calc(var(--nav-h) + 120px) 0 64px; }
.catpage__shade { position: absolute; inset: 0; background: linear-gradient(to top, var(--dg-900) 10%, transparent 70%), radial-gradient(ellipse at 20% 70%, color-mix(in srgb, var(--dg-900) 80%, transparent), transparent 70%); pointer-events: none; }
.catpage__head { position: relative; display: grid; gap: 28px; }
.catpage__counts { list-style: none; padding: 0; margin: 0; display: flex; flex-wrap: wrap; gap: 12px 32px; color: var(--text-2); font-size: 13px; }
.catpage__counts b { font-size: 28px; font-weight: 400; color: var(--accent); margin-right: 6px; vertical-align: -3px; }
.catpage__matrix { padding-top: 32px; }
.catpage__board { padding-top: 64px; }
.catpage__foot { padding: 120px var(--gutter); display: flex; flex-wrap: wrap; gap: 24px; align-items: center; justify-content: space-between; }
</style>
