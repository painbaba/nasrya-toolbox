<script setup lang="ts">
import { REPO } from '~/composables/catalog';
const demo = withBase('demo/');
const cols = [
  { h: 'Product', l: [['Features', '/#features'], ['Catalog', '/catalog'], ['Web view', '/#web-view'], ['Live demo', 'demo']] },
  { h: 'Docs', l: [['Quick start', '/docs/get-started/quickstart'], ['Keyboard', '/docs/using/keyboard'], ['tools.yaml', '/docs/catalog/tools-yaml'], ['Security model', '/docs/security/model']] },
  { h: 'Project', l: [['GitHub', REPO], ['Releases', `${REPO}/releases`], ['Issues', `${REPO}/issues`], ['License (MIT)', `${REPO}/blob/main/LICENSE`]] },
];
const href = (to: string) => (to === 'demo' ? demo : to);
const external = (to: string) => to.startsWith('http') || to === 'demo';
const year = new Date().getFullYear();
</script>

<template>
  <footer class="foot">
    <div class="wrap">
      <div class="foot__top">
        <div class="foot__brand">
          <LogoMark />
          <p>The native-Windows security toolbox. Find, install and run pentest &amp; CTF tools from one dashboard.</p>
        </div>
        <nav v-for="c in cols" :key="c.h" class="foot__col" :aria-label="c.h">
          <h4 class="mono">{{ c.h }}</h4>
          <template v-for="[label, to] in c.l" :key="label">
            <a v-if="external(to!)" :href="href(to!)">{{ label }}</a>
            <NuxtLink v-else :to="to">{{ label }}</NuxtLink>
          </template>
        </nav>
      </div>
      <div class="foot__word pixel" aria-hidden="true">WINSPLOIT</div>
      <div class="foot__bottom mono">
        <span>MIT © {{ year }} Winsploit contributors. Each installed tool keeps its own license.</span>
        <span>For authorized testing, research, CTFs and learning only.</span>
      </div>
    </div>
  </footer>
</template>

<style scoped>
.foot { border-top: 1px solid var(--line); padding: 80px 0 32px; overflow: hidden; }
.foot__top { display: grid; grid-template-columns: 2fr repeat(3, 1fr); gap: 40px; }
.foot__brand { display: grid; gap: 18px; align-content: start; max-width: 320px; color: var(--text-2); font-size: 14px; }
.foot__col { display: grid; gap: 10px; align-content: start; }
.foot__col h4 { font-size: 11px; letter-spacing: .14em; text-transform: uppercase; color: var(--text-3); font-weight: 400; margin-bottom: 6px; }
.foot__col a { color: var(--lg-300); font-size: 15px; width: fit-content; background: linear-gradient(var(--accent), var(--accent)) 0 100% / 0 1px no-repeat; transition: background-size .5s var(--ease-out), color .3s; }
.foot__col a:hover { color: var(--text); background-size: 100% 1px; }
.foot__word {
  margin: 80px 0 24px; font-size: clamp(56px, 15.6vw, 236px); line-height: .8; letter-spacing: -.02em; white-space: nowrap;
  color: transparent; -webkit-text-stroke: 1px var(--dg-400);
  background: linear-gradient(90deg, var(--accent) 0 var(--p, 0%), transparent var(--p, 0%)); -webkit-background-clip: text; background-clip: text;
  transition: --p 1.6s var(--ease-out);
}
.foot__word:hover { --p: 100%; }
@property --p { syntax: '<percentage>'; inherits: false; initial-value: 0%; }
.foot__bottom { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 12px; padding-top: 20px; border-top: 1px solid var(--line); font-size: 11.5px; color: var(--text-3); }
@media (max-width: 860px) { .foot__top { grid-template-columns: 1fr 1fr; } .foot__brand { grid-column: 1 / -1; } }
</style>
