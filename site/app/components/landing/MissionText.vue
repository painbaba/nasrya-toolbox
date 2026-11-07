<script setup lang="ts">
// A big paragraph whose words light up as it scrolls through the viewport.
const text = ref<HTMLElement>();
onMounted(() => {
  if (prefersReducedMotion() || !text.value) return;
  const { $gsap, $SplitText } = useNuxtApp();
  const split = $SplitText.create(text.value, { type: 'words', wordsClass: 'mission__w' });
  const tw = $gsap.fromTo(
    split.words,
    { opacity: 0.12 },
    {
      opacity: 1,
      ease: 'none',
      stagger: 0.1,
      scrollTrigger: { trigger: text.value, start: 'top 80%', end: 'bottom 45%', scrub: 0.6 },
    },
  );
  onBeforeUnmount(() => {
    tw.scrollTrigger?.kill();
    tw.kill();
    split.revert();
  });
});
</script>

<template>
  <section class="mission section">
    <div class="wrap mission__grid">
      <p class="eyebrow" v-reveal>Why Winsploit</p>
      <p ref="text" class="mission__text">
        Kali is the home of security tooling, but plenty of us work on Windows, and every option makes you
        compromise: a one-shot VM provisioner, a launcher abandoned years ago, or a package manager that has never
        heard of a pentest. Winsploit <em>installs and runs</em> the tools that really work on Windows, and it is
        honest about the ones that don't.
      </p>
    </div>
  </section>
</template>

<style scoped>
.mission__grid { display: grid; grid-template-columns: minmax(160px, 1fr) 4fr; gap: 32px; align-items: start; }
.mission__text { font-size: clamp(28px, 3.9vw, 60px); line-height: 1.08; letter-spacing: -.035em; color: var(--lg-50); }
.mission__text em { font-style: normal; color: var(--accent); }
@media (max-width: 860px) { .mission__grid { grid-template-columns: 1fr; } }
</style>
