<script setup lang="ts">
// Markdown links: internal pages go through the router; the demo (a static
// page outside the app) and external sites are plain links.
const props = defineProps<{ href?: string; target?: string }>();
const isStatic = computed(() => props.href?.startsWith('/demo'));
const isExternal = computed(() => /^https?:/.test(props.href ?? ''));
const staticHref = computed(() => (isStatic.value ? withBase(props.href!) : props.href));
</script>

<template>
  <a v-if="isExternal" :href="href" target="_blank" rel="noopener"><slot /></a>
  <a v-else-if="isStatic" :href="staticHref"><slot /></a>
  <NuxtLink v-else :to="href" :target="target"><slot /></NuxtLink>
</template>
