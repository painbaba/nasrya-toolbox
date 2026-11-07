import type Lenis from 'lenis';

/** True when the visitor asked for less motion (always false on the server). */
export function prefersReducedMotion(): boolean {
  return import.meta.client && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

let lenis: Lenis | null = null;
export const getLenis = () => lenis;

/** Smooth scrolling for the component's lifetime, driven by the GSAP ticker. */
export function useLenis() {
  const { $gsap, $ScrollTrigger } = useNuxtApp();
  let tick: ((t: number) => void) | null = null;
  onMounted(async () => {
    if (prefersReducedMotion()) return;
    const { default: L } = await import('lenis');
    lenis = new L({ lerp: 0.1, anchors: { offset: -80 }, autoRaf: false });
    lenis.on('scroll', $ScrollTrigger.update);
    tick = (t: number) => lenis?.raf(t * 1000);
    $gsap.ticker.add(tick);
    $gsap.ticker.lagSmoothing(0);
  });
  onBeforeUnmount(() => {
    if (tick) $gsap.ticker.remove(tick);
    lenis?.destroy();
    lenis = null;
  });
}

const GLYPHS = '!<>-_\/[]{}—=+*^?#01ABCDEF';

/** Scramble `el`'s text into `text`, left to right. Resolves when done. */
export function scramble(el: HTMLElement, text: string, duration = 900): Promise<void> {
  if (prefersReducedMotion()) {
    el.textContent = text;
    return Promise.resolve();
  }
  return new Promise((done) => {
    const start = performance.now();
    const frame = (now: number) => {
      const p = Math.min(1, (now - start) / duration);
      const fixed = Math.floor(p * text.length);
      let s = text.slice(0, fixed);
      for (let i = fixed; i < text.length; i++) {
        s += text[i] === ' ' ? ' ' : GLYPHS[(Math.random() * GLYPHS.length) | 0];
      }
      el.textContent = s;
      if (p < 1) requestAnimationFrame(frame);
      else done();
    };
    requestAnimationFrame(frame);
  });
}

/** Run `fn` once the element is on screen. */
export function onVisible(el: Element, fn: () => void, margin = '0px 0px -15% 0px') {
  const io = new IntersectionObserver(
    (es) => {
      if (es.some((e) => e.isIntersecting)) {
        io.disconnect();
        fn();
      }
    },
    { rootMargin: margin },
  );
  io.observe(el);
  return () => io.disconnect();
}
