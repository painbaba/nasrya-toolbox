<script setup lang="ts">
// The Winsploit mark: the Iso cube, an isometric cube with a prompt on its top
// face (32 × 32 grid; paths and colours in design/README.md).
</script>

<template>
  <svg class="logo-mark" viewBox="0 0 32 32" aria-hidden="true">
    <path class="logo-mark__left" d="M16 2.5 28 9.3v13.4L16 29.5 4 22.7V9.3z" />
    <path class="logo-mark__top" d="M16 2.5 28 9.3 16 16 4 9.3z" />
    <path class="logo-mark__right" d="M16 16 28 9.3v13.4L16 29.5z" />
    <path class="logo-mark__prompt" d="M10.5 8l3 1.6-3 1.7" />
    <path class="logo-mark__cursor" d="M15 11.5l3.2 1.8" />
  </svg>
</template>

<style scoped>
.logo-mark { width: 26px; height: 26px; overflow: visible; flex: none; }
.logo-mark__left { fill: var(--bg-700); }
.logo-mark__top { fill: var(--bg-200); }
.logo-mark__right { fill: var(--bg-500); }
.logo-mark__prompt,
.logo-mark__cursor { fill: none; stroke: var(--accent-ink); stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
.logo-mark__cursor { animation: blink 1.2s steps(1) infinite; }
@keyframes blink { 50% { opacity: 0; } }
@media (prefers-reduced-motion: reduce) { .logo-mark__cursor { animation: none; } }
</style>
