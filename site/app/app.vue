<script setup lang="ts">
useHead({
  // mark JS early so reveal styles never hide content for no-JS visitors
  script: [{ innerHTML: "document.documentElement.classList.add('js')", tagPosition: 'head' }],
});
</script>

<template>
  <NuxtLayout>
    <NuxtPage />
  </NuxtLayout>
</template>
