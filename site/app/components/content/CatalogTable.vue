<script setup lang="ts">
// The full catalog as a filterable table (docs: Catalog › Overview).
const cat = useCatalog();
const q = ref('');
const route = useRoute();
const only = ref<string>((route.query.category as string) || '');
const label = (id: string) => cat.categories.find((c) => c.id === id)?.label ?? id;
const rows = computed(() =>
  cat.tools.filter((t) => {
    if (only.value && !t.categories.includes(only.value)) return false;
    const hay = `${t.name} ${t.description} ${t.categories.join(' ')}`.toLowerCase();
    return q.value.toLowerCase().split(/\s+/).filter(Boolean).every((w) => hay.includes(w));
  }),
);
const via = (t: (typeof cat.tools)[number]) => (t.install === 'pipx' ? `pipx · ${t.package}` : `scoop · ${t.bucket === 'main' ? '' : t.bucket + '/'}${t.package}`);
</script>

<template>
  <div class="ct">
    <div class="ct__bar">
      <input v-model="q" class="ct__q" type="search" placeholder="Filter tools…" spellcheck="false" aria-label="Filter tools" />
      <select v-model="only" class="ct__sel mono" aria-label="Category">
        <option value="">All categories</option>
        <option v-for="c in cat.categories" :key="c.id" :value="c.id">{{ c.label }} ({{ c.tools.length }})</option>
      </select>
      <span class="ct__n mono">{{ rows.length }} / {{ cat.tools.length }}</span>
    </div>
    <div class="ct__wrap">
      <table>
        <thead><tr><th>Tool</th><th>Categories</th><th>Launch</th><th>Installs via</th><th>What it does</th></tr></thead>
        <tbody>
          <tr v-for="t in rows" :key="t.name">
            <td><b>{{ t.name }}</b><span v-if="t.verify" class="ct__flag mono" title="verify: impostor check">verify</span><span v-if="t.note" class="ct__flag ct__flag--w mono" :title="t.note">note</span></td>
            <td class="mono ct__cats">{{ t.categories.map(label).join(' · ') }}</td>
            <td class="mono">{{ t.launch }}</td>
            <td class="mono ct__via">{{ via(t) }}</td>
            <td>{{ t.description }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.ct { margin-top: 1.4em; border: 1px solid var(--line-strong); border-radius: 10px; overflow: hidden; }
.ct__bar { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; padding: 10px; border-bottom: 1px solid var(--line); background: var(--dg-800); }
.ct__q { flex: 1 1 200px; height: 34px; padding: 0 10px; border: 1px solid var(--line-strong); border-radius: 6px; background: var(--dg-900); color: var(--text); font: 14px var(--font-sans); outline: none; }
.ct__q:focus { border-color: var(--accent); }
.ct__sel { height: 34px; padding: 0 8px; border: 1px solid var(--line-strong); border-radius: 6px; background: var(--dg-900); color: var(--text-2); font-size: 12.5px; }
.ct__n { font-size: 12px; color: var(--text-3); padding: 0 6px; }
.ct__wrap { max-height: 560px; overflow: auto; }
table { display: table !important; width: 100%; border-collapse: collapse; font-size: 13.5px; }
thead th { position: sticky; top: 0; background: var(--dg-700); z-index: 1; padding: 10px 12px !important; font-size: 12px; }
td { padding: 9px 12px !important; }
td b { color: var(--text); font-weight: 500; }
.ct__cats, .ct__via { font-size: 12px; color: var(--text-2); white-space: nowrap; }
.ct__flag { margin-left: 6px; font-size: 10px; padding: 0 5px; border-radius: 3px; background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--accent); }
.ct__flag--w { background: color-mix(in srgb, var(--warn) 14%, transparent); color: var(--warn); }
</style>
