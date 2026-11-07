export interface NavPage { path: string; title: string; group: string; badge?: string }

/** Every docs page in sidebar order (file prefixes), grouped by `group`. */
export async function useDocsNav() {
  const { data } = await useAsyncData('docs-nav', () =>
    queryCollection('docs').select('path', 'title', 'group', 'badge', 'stem').order('stem', 'ASC').all(),
  );
  const pages = computed<NavPage[]>(() => (data.value ?? []) as NavPage[]);
  const groups = computed(() => {
    const out: { group: string; pages: NavPage[] }[] = [];
    for (const p of pages.value) {
      let g = out.find((x) => x.group === p.group);
      if (!g) out.push((g = { group: p.group, pages: [] }));
      g.pages.push(p);
    }
    return out;
  });
  return { pages, groups };
}
