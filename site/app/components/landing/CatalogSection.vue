<script setup lang="ts">
const cat = useCatalog();
const explorer = ref<{ setCat: (id: string) => void }>();
const boardTop = ref<HTMLElement>();
function pick(id: string) {
  explorer.value?.setCat(id);
  boardTop.value?.scrollIntoView({ behavior: 'smooth', block: 'start' });
}
</script>

<template>
  <section id="catalog" class="catalog section">
    <div class="wrap">
      <div class="catalog__head">
        <div class="section-head">
          <p class="eyebrow" v-reveal>The catalog</p>
          <h2 class="h-lg" v-reveal="0.05">
            {{ cat.counts.tools }} tools. {{ cat.counts.categories }} categories.<br />
            <span class="accent">All native.</span>
          </h2>
        </div>
        <p class="lead" v-reveal="0.1">
          Every tool was picked because it runs well on Windows itself. Each one is a few lines of YAML: what to
          detect, how to install it, what Enter runs. Here is all of it, straight from
          <code>tools.yaml</code>.
        </p>
      </div>

      <div class="catalog__matrix" v-reveal="0.1">
        <CategoryMatrix @pick="pick" />
      </div>

      <div ref="boardTop" class="catalog__board" v-reveal>
        <CatalogExplorer ref="explorer" compact />
      </div>

      <div class="catalog__more" v-reveal>
        <NuxtLink to="/catalog" class="btn btn--primary">Open the full catalog <ArrowIcon /></NuxtLink>
        <NuxtLink to="/docs/catalog/adding-a-tool" class="btn">Add your own tool <ArrowIcon /></NuxtLink>
      </div>
    </div>
  </section>
</template>

<style scoped>
.catalog__head { display: grid; grid-template-columns: 1.4fr 1fr; gap: 40px; align-items: end; }
.catalog__head code { font-size: .9em; color: var(--accent); }
.catalog__matrix { margin-top: 72px; padding: 28px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--dg-900); }
.catalog__board { margin-top: 56px; scroll-margin-top: calc(var(--nav-h) + 24px); }
.catalog__more { margin-top: 48px; display: flex; flex-wrap: wrap; gap: 12px; }
@media (max-width: 980px) { .catalog__head { grid-template-columns: 1fr; } .catalog__matrix { padding: 16px; } }
</style>
