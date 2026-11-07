<script setup lang="ts">
// Tools × categories as a pixel matrix: one column per tool, one row per
// category, a lit cell where the tool is listed. Shows how the catalog
// overlaps (a tool can live in several categories). Lights up in a wave when
// it scrolls in; hovering highlights a row and a column.
const cat = useCatalog();
const emit = defineEmits<{ pick: [id: string] }>();
const hoverTool = ref<number | null>(null);
const hoverCat = ref<string | null>(null);
const root = ref<HTMLElement>();
const lit = ref(false);

const cols = cat.tools;
onMounted(() => {
  if (prefersReducedMotion()) {
    lit.value = true;
    return;
  }
  onVisible(root.value!, () => (lit.value = true));
});
const label = computed(() => {
  if (hoverTool.value !== null) {
    const t = cols[hoverTool.value]!;
    return `${t.name} — ${t.description}`;
  }
  if (hoverCat.value) {
    const c = cat.categories.find((x) => x.id === hoverCat.value)!;
    return `${c.label} — ${c.blurb} (${c.tools.length})`;
  }
  return 'Hover a cell to see the tool · click a row to filter the board below';
});
</script>

<template>
  <div ref="root" class="mx" :class="{ 'is-lit': lit }">
    <div class="mx__scroll">
      <table class="mx__table" @mouseleave="hoverTool = null; hoverCat = null">
        <tbody>
          <tr
            v-for="(c, r) in cat.categories"
            :key="c.id"
            :class="{ 'is-row': hoverCat === c.id }"
            @mouseenter="hoverCat = c.id"
            @click="emit('pick', c.id)"
          >
            <th scope="row" class="mono">{{ c.label }}</th>
            <td
              v-for="(t, k) in cols"
              :key="t.name"
              :class="{ on: t.categories.includes(c.id), primary: t.primary === c.id, 'is-col': hoverTool === k }"
              :style="{ '--d': `${(r * 0.035 + k * 0.012).toFixed(3)}s` }"
              @mouseenter="hoverTool = k"
            />
            <td class="mx__count mono">{{ c.tools.length }}</td>
          </tr>
        </tbody>
      </table>
    </div>
    <p class="mx__label mono" aria-live="polite"><span class="t-ok">›</span> {{ label }}</p>
    <p class="mx__legend mono">
      <span><i class="lg lg--p" /> primary category</span>
      <span><i class="lg" /> also listed in</span>
      <span>{{ cat.counts.tools }} columns = {{ cat.counts.tools }} tools</span>
    </p>
  </div>
</template>

<style scoped>
.mx { display: grid; gap: 14px; }
.mx__scroll { overflow-x: auto; scrollbar-width: thin; padding-bottom: 6px; mask-image: linear-gradient(90deg, #000 92%, transparent); }
.mx__table { border-collapse: separate; border-spacing: 3px; cursor: crosshair; }
.mx__table th { position: sticky; left: 0; z-index: 1; background: var(--dg-900); text-align: left; font-weight: 400; font-size: 11.5px; letter-spacing: .06em; text-transform: uppercase; color: var(--text-3); padding-right: 14px; white-space: nowrap; cursor: pointer; transition: color .25s; }
.mx__table tr.is-row th { color: var(--accent); }
.mx__table td { width: 15px; height: 15px; min-width: 15px; padding: 0; background: var(--dg-700); border-radius: 1px; transition: background-color .5s var(--ease-out), box-shadow .5s var(--ease-out), opacity .3s; }
.is-lit .mx__table td.on { background: var(--bg-800); transition-delay: var(--d); }
.is-lit .mx__table td.primary { background: var(--accent); box-shadow: 0 0 8px var(--accent-glow); }
.mx__table tr.is-row td:not(.mx__count), .mx__table td.is-col { outline: 1px solid color-mix(in srgb, var(--accent) 35%, transparent); }
.mx__table tr.is-row td.on, .mx__table td.is-col.on { background: var(--bg-200); transition-delay: 0s; }
.mx__table td.mx__count { background: none; outline: none !important; font-size: 11px; color: var(--text-3); padding-left: 10px; width: auto; }
.mx__label { min-height: 1.6em; font-size: 13px; color: var(--lg-300); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.mx__legend { display: flex; flex-wrap: wrap; gap: 8px 22px; font-size: 11px; color: var(--text-3); }
.mx__legend span { display: inline-flex; align-items: center; gap: 8px; }
.lg { width: 10px; height: 10px; background: var(--bg-800); display: inline-block; }
.lg--p { background: var(--accent); }
</style>
