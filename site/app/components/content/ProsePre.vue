<script setup lang="ts">
// Code blocks: a header with the filename or language and a copy button.
const props = defineProps<{
  code?: string;
  language?: string | null;
  filename?: string | null;
  highlights?: number[];
  meta?: string | null;
  class?: string | null;
}>();
const copied = ref(false);
async function copy() {
  try {
    await navigator.clipboard.writeText(props.code ?? '');
    copied.value = true;
    setTimeout(() => (copied.value = false), 1400);
  } catch {
    /* clipboard blocked */
  }
}
const LANG: Record<string, string> = { powershell: 'PowerShell', ps1: 'PowerShell', yaml: 'YAML', bash: 'Shell', sh: 'Shell', json: 'JSON', rust: 'Rust', text: 'Text', ini: 'INI' };
const label = computed(() => props.filename || LANG[props.language ?? ''] || props.language || 'Text');
</script>

<template>
  <div class="code">
    <div class="code__bar">
      <span class="code__label mono">{{ label }}</span>
      <button class="code__copy mono" type="button" :aria-label="`Copy ${label}`" @click="copy">{{ copied ? 'Copied ✓' : 'Copy' }}</button>
    </div>
    <pre :class="props.class"><slot /></pre>
  </div>
</template>

<style scoped>
.code { margin-top: 1.3em; border: 1px solid var(--line-strong); border-radius: 10px; overflow: hidden; background: var(--dg-800); }
.code__bar { display: flex; justify-content: space-between; align-items: center; height: 38px; padding: 0 8px 0 14px; border-bottom: 1px solid var(--line); background: var(--dg-700); }
.code__label { font-size: 12px; color: var(--text-3); }
.code__copy { font-size: 11.5px; color: var(--text-2); padding: 4px 8px; border-radius: 4px; transition: color .2s, background-color .2s; }
.code__copy:hover { color: var(--accent); background: var(--surface-3); }
pre { margin: 0; padding: 16px 18px; overflow-x: auto; font-size: 13.5px; line-height: 1.7; background: transparent !important; }
pre :deep(code) { font-family: var(--font-mono); }
pre :deep(.line) { display: block; min-height: 1.7em; }
</style>
