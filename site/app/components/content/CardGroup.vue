<script setup lang="ts">
withDefaults(defineProps<{ cols?: number }>(), { cols: 2 });
</script>

<template>
  <div class="cgrp" :style="{ '--cols': cols }"><slot /></div>
</template>

<style scoped>
.cgrp { margin-top: 1.4em; display: grid; grid-template-columns: repeat(var(--cols), minmax(0, 1fr)); gap: 12px; }
@media (max-width: 720px) { .cgrp { grid-template-columns: 1fr; } }
</style>
