<script setup lang="ts">
// "On this page", with the heading in view highlighted.
interface Link { id: string; text: string; depth: number; children?: Link[] }
const props = defineProps<{ links: Link[] }>();
const flat = computed(() => props.links.flatMap((l) => [l, ...(l.children ?? [])]));
const active = ref('');
let io: IntersectionObserver | null = null;

function observe() {
  io?.disconnect();
  const seen = new Map<string, boolean>();
  io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) seen.set(e.target.id, e.isIntersecting);
      const first = flat.value.find((l) => seen.get(l.id));
      if (first) active.value = first.id;
    },
    { rootMargin: '-70px 0px -65% 0px' },
  );
  for (const l of flat.value) {
    const el = document.getElementById(l.id);
    if (el) io.observe(el);
  }
}
onMounted(observe);
watch(flat, () => nextTick(observe));
onBeforeUnmount(() => io?.disconnect());
</script>

<template>
  <nav v-if="flat.length" class="toc" aria-label="On this page">
    <p class="mono">On this page</p>
    <ul>
      <li v-for="l in flat" :key="l.id" :class="[`d${l.depth}`, { 'is-on': active === l.id }]">
        <a :href="`#${l.id}`">{{ l.text }}</a>
      </li>
    </ul>
  </nav>
</template>

<style scoped>
.toc { position: sticky; top: 92px; max-height: calc(100vh - 120px); overflow-y: auto; font-size: 13px; }
.toc p { font-size: 11px; letter-spacing: .12em; text-transform: uppercase; color: var(--text-3); margin-bottom: 10px; }
.toc ul { list-style: none; margin: 0; padding: 0; border-left: 1px solid var(--line); }
.toc li a { display: block; padding: 4px 0 4px 14px; margin-left: -1px; border-left: 1px solid transparent; color: var(--text-3); line-height: 1.4; transition: color .2s, border-color .2s; }
.toc li.d3 a { padding-left: 26px; }
.toc li a:hover { color: var(--text); }
.toc li.is-on a { color: var(--accent); border-left-color: var(--accent); }
</style>
