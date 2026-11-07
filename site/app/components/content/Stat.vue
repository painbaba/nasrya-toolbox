<script setup lang="ts">
// Inline catalog number in Markdown: :stat{k="tools"} → 45. Keeps docs from
// hardcoding counts that change whenever tools.yaml does.
const props = defineProps<{ k: string }>();
const cat = useCatalog();
const v = computed(() => {
  if (props.k === 'slots') return cat.slots;
  if (props.k === 'hashFormats') return cat.hashFormats;
  if (props.k === 'version') return cat.version;
  return cat.counts[props.k] ?? '?';
});
</script>

<template><span>{{ v }}</span></template>
