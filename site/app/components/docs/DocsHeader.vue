<script setup lang="ts">
import { REPO } from '~/composables/catalog';
const demo = withBase('demo/');
const search = useState('docs-search', () => false);
const menu = useState('docs-menu', () => false);
const isMac = ref(false);
onMounted(() => (isMac.value = /Mac|iPhone|iPad/.test(navigator.platform)));
</script>

<template>
  <header class="dh">
    <div class="dh__inner">
      <button class="dh__menu" aria-label="Docs menu" :aria-expanded="menu" @click="menu = !menu"><i /><i /><i /></button>
      <NuxtLink to="/" class="dh__brand" aria-label="Winsploit home">
        <LogoMark />
        <span class="pixel">WINSPLOIT</span>
      </NuxtLink>
      <NuxtLink to="/docs" class="dh__docs mono">Docs</NuxtLink>
      <button class="dh__search" @click="search = true">
        <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" /></svg>
        <span>Search docs…</span>
        <kbd>{{ isMac ? '⌘' : 'Ctrl' }} K</kbd>
      </button>
      <nav class="dh__links mono">
        <NuxtLink to="/catalog">Catalog</NuxtLink>
        <a :href="demo">Live demo</a>
        <a :href="REPO" target="_blank" rel="noopener"><GithubIcon /> GitHub</a>
      </nav>
    </div>
    <SearchModal />
  </header>
</template>

<style scoped>
.dh { position: sticky; top: 0; z-index: 40; height: 60px; border-bottom: 1px solid var(--line); background: color-mix(in srgb, var(--dg-900) 85%, transparent); backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px); }
.dh__inner { height: 100%; display: flex; align-items: center; gap: 20px; padding: 0 20px; max-width: 1600px; margin: 0 auto; }
.dh__brand { display: flex; align-items: center; gap: 10px; font-size: 14px; letter-spacing: .06em; }
.dh__docs { font-size: 12px; letter-spacing: .1em; text-transform: uppercase; color: var(--accent); padding: 3px 8px; border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent); border-radius: 3px; }
.dh__search {
  margin-left: 24px; flex: 0 1 380px; display: flex; align-items: center; gap: 10px; height: 36px; padding: 0 8px 0 12px;
  border: 1px solid var(--line-strong); border-radius: 6px; background: var(--dg-800); color: var(--text-3); font-size: 14px; text-align: left;
  transition: border-color .3s var(--ease-out);
}
.dh__search:hover { border-color: var(--lg-800); }
.dh__search svg { width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 2; }
.dh__search span { flex: 1; }
.dh__links { margin-left: auto; display: flex; gap: 22px; font-size: 13px; color: var(--text-2); }
.dh__links a { display: inline-flex; align-items: center; gap: 6px; transition: color .3s; }
.dh__links a:hover { color: var(--text); }
.dh__menu { display: none; width: 36px; height: 36px; position: relative; margin-left: -6px; }
.dh__menu i { position: absolute; left: 9px; right: 9px; height: 1.5px; background: var(--text); }
.dh__menu i:nth-child(1) { top: 12px; } .dh__menu i:nth-child(2) { top: 17px; } .dh__menu i:nth-child(3) { top: 22px; }
@media (max-width: 1024px) { .dh__links { display: none; } .dh__menu { display: block; } }
@media (max-width: 640px) {
  .dh__inner { padding: 0 12px; gap: 10px; }
  .dh__brand span, .dh__search span, .dh__search kbd { display: none; }
  .dh__search { margin-left: auto; flex: 0 0 36px; padding: 0; justify-content: center; }
}
</style>
