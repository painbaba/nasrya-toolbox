<script setup lang="ts">
const cat = useCatalog();
useLenis();
useSeoMeta({
  title: 'Winsploit — pentest tools, native on Windows',
  ogTitle: 'Winsploit — pentest tools, native on Windows',
  description: `Find, install and run ${cat.counts.tools} curated pentest and CTF tools natively on Windows, from one keyboard-driven terminal dashboard or your browser. No WSL. No VM.`,
  ogDescription: `Find, install and run ${cat.counts.tools} curated pentest and CTF tools natively on Windows. No WSL. No VM.`,});
</script>

<template>
  <main>
    <HeroSection />
    <ToolTicker />
    <MissionText />
    <HelloSection />
    <StatsBand />
    <PillarsSection />
    <CatalogSection />
    <WebViewSection />
    <SecuritySection />
    <YamlSection />
    <ClosingCta />
  </main>
</template>
