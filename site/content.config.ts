import { defineCollection, defineContentConfig, z } from '@nuxt/content';

export default defineContentConfig({
  collections: {
    docs: defineCollection({
      type: 'page',
      source: 'docs/**/*.md',
      schema: z.object({
        // sidebar group heading; pages keep the order of their file prefix
        group: z.string(),
        badge: z.string().optional(),
      }),
    }),
  },
});
