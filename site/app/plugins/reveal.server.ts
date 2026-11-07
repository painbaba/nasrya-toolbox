// The directive exists on the server too, so SSR renders the element as-is.
export default defineNuxtPlugin((nuxtApp) => {
  nuxtApp.vueApp.directive('reveal', { getSSRProps: () => ({ 'data-reveal': '' }) });
});
