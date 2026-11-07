// GSAP with ScrollTrigger + SplitText, and the v-reveal directive.
import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { SplitText } from 'gsap/SplitText';
import { Flip } from 'gsap/Flip';

export default defineNuxtPlugin((nuxtApp) => {
  gsap.registerPlugin(ScrollTrigger, SplitText, Flip);
  gsap.defaults({ ease: 'expo.out', duration: 1.1 });

  // v-reveal: fade + rise once the element scrolls in. v-reveal="0.2" delays it.
  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        e.target.classList.add('is-in');
        io.unobserve(e.target);
      }
    },
    { rootMargin: '0px 0px -12% 0px' },
  );
  nuxtApp.vueApp.directive('reveal', {
    mounted(el: HTMLElement, binding) {
      el.setAttribute('data-reveal', '');
      if (binding.value) el.style.setProperty('--reveal-delay', `${binding.value}s`);
      io.observe(el);
    },
    unmounted(el: HTMLElement) {
      io.unobserve(el);
    },
  });

  return { provide: { gsap, ScrollTrigger, SplitText, Flip } };
});
