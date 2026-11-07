<script setup lang="ts">
const props = defineProps<{ title: string; to?: string; icon?: string }>();
const external = computed(() => props.to?.startsWith('http'));
const tag = computed(() => (props.to ? (external.value ? 'a' : resolveComponent('NuxtLink')) : 'div'));
const attrs = computed(() => (props.to ? (external.value ? { href: props.to, target: '_blank', rel: 'noopener' } : { to: props.to }) : {}));
</script>

<template>
  <component :is="tag" v-bind="attrs" class="card" :class="{ 'card--link': to }">
    <span v-if="icon" class="card__icon mono" aria-hidden="true">{{ icon }}</span>
    <span class="card__title">{{ title }}<span v-if="to" class="card__arrow">→</span></span>
    <span class="card__text"><slot /></span>
  </component>
</template>

<style scoped>
.card { display: grid; gap: 6px; align-content: start; padding: 18px; border: 1px solid var(--line); border-radius: 10px; background: var(--dg-800); text-decoration: none !important; color: var(--text-2) !important; transition: border-color .3s var(--ease-out), background-color .3s var(--ease-out), transform .4s var(--ease-out); }
.card--link:hover { border-color: color-mix(in srgb, var(--accent) 50%, transparent); background: var(--dg-700); transform: translateY(-2px); }
.card__icon { display: grid; place-items: center; width: 32px; height: 32px; border-radius: 8px; background: color-mix(in srgb, var(--accent) 12%, transparent); color: var(--accent); font-size: 15px; margin-bottom: 6px; }
.card__title { color: var(--text); font-weight: 500; font-size: 16px; display: flex; justify-content: space-between; gap: 8px; }
.card__arrow { color: var(--accent); transition: transform .3s var(--ease-out); }
.card--link:hover .card__arrow { transform: translateX(3px); }
.card__text { font-size: 14px; line-height: 1.5; }
.card__text :deep(p) { margin: 0; }
</style>
