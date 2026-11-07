<script setup lang="ts">
const props = withDefaults(defineProps<{ type?: 'note' | 'tip' | 'warning' | 'important' | 'caution'; title?: string }>(), { type: 'note' });
const META = {
  note: { icon: 'i', label: 'Note' },
  tip: { icon: '✓', label: 'Tip' },
  warning: { icon: '!', label: 'Warning' },
  important: { icon: '★', label: 'Important' },
  caution: { icon: '✕', label: 'Caution' },
} as const;
const m = computed(() => META[props.type]);
</script>

<template>
  <aside class="callout" :class="`callout--${type}`">
    <span class="callout__icon mono" aria-hidden="true">{{ m.icon }}</span>
    <div class="callout__body">
      <p class="callout__title mono">{{ title ?? m.label }}</p>
      <slot />
    </div>
  </aside>
</template>

<style scoped>
.callout { --c: var(--syn-key); display: flex; gap: 14px; margin-top: 1.4em; padding: 14px 16px; border: 1px solid color-mix(in srgb, var(--c) 30%, transparent); border-radius: 10px; background: color-mix(in srgb, var(--c) 6%, var(--dg-900)); }
.callout--tip { --c: var(--accent); }
.callout--warning { --c: var(--warn); }
.callout--important { --c: var(--amber); }
.callout--caution { --c: var(--risk); }
.callout__icon { flex: none; display: grid; place-items: center; width: 22px; height: 22px; margin-top: 2px; border-radius: 50%; background: var(--c); color: var(--dg-900); font-size: 12px; font-weight: 700; }
.callout__title { font-size: 11.5px; letter-spacing: .1em; text-transform: uppercase; color: var(--c); margin-bottom: 2px; }
.callout__body { min-width: 0; font-size: 15px; }
.callout__body :deep(p + p) { margin-top: .6em; }
.callout__body :deep(.code) { margin-top: .8em; }
</style>
