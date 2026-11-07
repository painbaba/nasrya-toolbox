<script setup lang="ts">
// Tabs over the code blocks inside it; each block's filename is its tab.
import type { VNode } from 'vue';
const slots = useSlots();
const active = ref(0);
const blocks = computed(() => (slots.default?.() ?? []).filter((v: VNode) => v.props));
const label = (v: VNode, i: number) => (v.props as any)?.filename || (v.props as any)?.language || `Tab ${i + 1}`;
</script>

<template>
  <div class="cg">
    <div class="cg__tabs" role="tablist">
      <button v-for="(b, i) in blocks" :key="i" role="tab" :aria-selected="active === i" class="mono" :class="{ 'is-on': active === i }" @click="active = i">{{ label(b, i) }}</button>
    </div>
    <div class="cg__body">
      <component :is="b" v-for="(b, i) in blocks" v-show="active === i" :key="i" />
    </div>
  </div>
</template>

<style scoped>
.cg { margin-top: 1.3em; border: 1px solid var(--line-strong); border-radius: 10px; overflow: hidden; background: var(--dg-800); }
.cg__tabs { display: flex; gap: 2px; padding: 0 8px; border-bottom: 1px solid var(--line); background: var(--dg-700); overflow-x: auto; }
.cg__tabs button { position: relative; height: 40px; padding: 0 12px; font-size: 12.5px; color: var(--text-3); white-space: nowrap; }
.cg__tabs button:hover { color: var(--text); }
.cg__tabs button.is-on { color: var(--accent); }
.cg__tabs button.is-on::after { content: ''; position: absolute; left: 10px; right: 10px; bottom: -1px; height: 2px; background: var(--accent); }
.cg__body :deep(.code) { margin: 0; border: 0; border-radius: 0; }
/* the copy button moves up into the tab row */
.cg__body :deep(.code__bar) { position: absolute; top: -40px; right: 0; height: 40px; border: 0; background: none; }
.cg__body :deep(.code__label) { display: none; }
.cg__body { position: relative; }
</style>
