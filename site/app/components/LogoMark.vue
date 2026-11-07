<script setup lang="ts">
// The Winsploit mark: a 7×5 pixel W with a red "risk" pixel.
const rows = ['X.....X', 'X.....X', 'X..X..X', 'X.X.X.X', '.X...X.'];
const cells = rows.flatMap((r, y) => [...r].flatMap((c, x) => (c === 'X' ? [{ x: x * 4, y: y * 4 }] : [])));
</script>

<template>
  <svg class="logo-mark" viewBox="0 0 30 20" aria-hidden="true">
    <rect v-for="(c, i) in cells" :key="i" :x="c.x" :y="c.y" width="3.4" height="3.4" fill="currentColor" :style="{ '--i': i }" />
    <rect class="logo-mark__risk" x="26.6" y="16" width="3.4" height="3.4" />
  </svg>
</template>

<style scoped>
.logo-mark { width: 30px; height: 20px; color: var(--accent); overflow: visible; flex: none; }
.logo-mark__risk { fill: var(--risk); animation: risk 2.4s steps(1) infinite; }
@keyframes risk { 0%, 70% { opacity: 1; } 71%, 85% { opacity: 0; } 86% { opacity: 1; } }
</style>
