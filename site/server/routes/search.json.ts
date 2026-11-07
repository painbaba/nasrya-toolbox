// Prerendered to /search.json: every docs section, for the Ctrl+K search.
export default defineEventHandler(async (event) => {
  return queryCollectionSearchSections(event, 'docs', { ignoredTags: ['style'] });
});
