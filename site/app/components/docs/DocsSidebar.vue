<script setup lang="ts">
import type { NavPage } from '~/composables/docsNav';
defineProps<{ groups: { group: string; pages: NavPage[] }[] }>();
const route = useRoute();
const menu = useState('docs-menu', () => false);
watch(() => route.path, () => (menu.value = false));
const ICON: Record<string, string> = {
  'Get started': '▶',
  'Using Winsploit': '⌨',
  'Web view': '◍',
  Catalog: '▦',
  Reference: '≡',
  'Security & project': '⛨',
};
</script>

<template>
  <aside class="ds" :class="{ 'is-open': menu }">
    <nav aria-label="Docs">
      <NuxtLink to="/docs" class="ds__home" :class="{ 'is-on': route.path === '/docs' }">Docs home</NuxtLink>
      <section v-for="g in groups" :key="g.group" class="ds__group">
        <h4 class="mono"><span class="ds__icon">{{ ICON[g.group] ?? '·' }}</span>{{ g.group }}</h4>
        <ul>
          <li v-for="p in g.pages" :key="p.path">
            <NuxtLink :to="p.path" :class="{ 'is-on': route.path === p.path }">
              {{ p.title }}
              <span v-if="p.badge" class="ds__badge mono">{{ p.badge }}</span>
            </NuxtLink>
          </li>
        </ul>
      </section>
      <NuxtLink to="/catalog" class="ds__cta">
        <span class="mono">Browse</span>
        <b>The tool catalog →</b>
      </NuxtLink>
    </nav>
  </aside>
  <div v-if="menu" class="ds__scrim" @click="menu = false" />
</template>

<style scoped>
.ds { position: sticky; top: 60px; height: calc(100vh - 60px); overflow-y: auto; padding: 24px 12px 48px 20px; border-right: 1px solid var(--line); scrollbar-width: thin; }
.ds__home { display: block; padding: 6px 10px; margin-bottom: 14px; border-radius: 6px; font-size: 14px; color: var(--text-2); }
.ds__home:hover, .ds__home.is-on { color: var(--text); background: var(--surface-2); }
.ds__group { margin-bottom: 22px; }
.ds__group h4 { display: flex; align-items: center; gap: 8px; font-size: 11px; font-weight: 500; letter-spacing: .12em; text-transform: uppercase; color: var(--text-3); padding: 0 10px; margin-bottom: 6px; }
.ds__icon { color: var(--accent); font-size: 12px; width: 14px; text-align: center; }
.ds__group ul { list-style: none; margin: 0; padding: 0; }
.ds__group a { position: relative; display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 5px 10px 5px 32px; border-radius: 6px; font-size: 14px; color: var(--text-2); transition: color .2s, background-color .2s; }
.ds__group a:hover { color: var(--text); background: var(--surface-2); }
.ds__group a.is-on { color: var(--accent); background: color-mix(in srgb, var(--accent) 9%, transparent); }
.ds__group a.is-on::before { content: ''; position: absolute; left: 16px; top: 50%; width: 5px; height: 5px; margin-top: -2.5px; background: var(--accent); box-shadow: 0 0 8px var(--accent); }
.ds__badge { font-size: 9.5px; padding: 1px 5px; border-radius: 3px; background: color-mix(in srgb, var(--accent) 15%, transparent); color: var(--accent); letter-spacing: .06em; text-transform: uppercase; }
.ds__cta { display: grid; gap: 4px; margin: 12px 10px 0; padding: 14px; border: 1px solid var(--line); border-radius: 8px; background: linear-gradient(135deg, color-mix(in srgb, var(--accent) 8%, transparent), transparent); }
.ds__cta span { font-size: 10.5px; letter-spacing: .12em; text-transform: uppercase; color: var(--text-3); }
.ds__cta b { font-weight: 500; font-size: 14px; color: var(--accent); }
.ds__scrim { display: none; }
@media (max-width: 1024px) {
  .ds { position: fixed; z-index: 45; left: 0; top: 60px; width: min(320px, 86vw); background: var(--dg-900); transform: translateX(-100%); transition: transform .5s var(--ease-out); }
  .ds.is-open { transform: none; }
  .ds__scrim { display: block; position: fixed; inset: 60px 0 0; z-index: 44; background: rgba(0, 0, 0, .5); }
}
</style>
