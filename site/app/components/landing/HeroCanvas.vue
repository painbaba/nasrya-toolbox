<script setup lang="ts">
// Full-bleed WebGL pixel field: a dithered noise grid in deep green that lights
// up around the cursor, a vertical "cursor stripe", and the odd red "risk"
// cell. Pauses off-screen and in background tabs; reduced motion draws one
// still frame; no WebGL falls back to a CSS grid.
const props = withDefaults(defineProps<{ intensity?: number; cell?: number; interactive?: boolean }>(), {
  intensity: 1,
  cell: 13,
  interactive: true,
});

const canvas = ref<HTMLCanvasElement>();
const fallback = ref(false);

const VERT = `attribute vec2 p; void main(){ gl_Position = vec4(p, 0., 1.); }`;
const FRAG = `
precision highp float;
uniform vec2 uRes;
uniform float uTime;
uniform vec2 uMouse;
uniform float uHover;
uniform float uCell;
uniform float uIntro;
uniform float uIntensity;

float hash(vec2 p){ p = fract(p * vec2(123.34, 456.21)); p += dot(p, p + 45.32); return fract(p.x * p.y); }
float noise(vec2 p){
  vec2 i = floor(p), f = fract(p);
  vec2 u = f * f * (3. - 2. * f);
  return mix(mix(hash(i), hash(i + vec2(1., 0.)), u.x), mix(hash(i + vec2(0., 1.)), hash(i + vec2(1., 1.)), u.x), u.y);
}
float fbm(vec2 p){ float v = 0., a = .5; for (int i = 0; i < 4; i++){ v += a * noise(p); p *= 2.03; a *= .5; } return v; }
float bayer2(vec2 a){ a = floor(a); return fract(a.x / 2. + a.y * a.y * .75); }
float bayer4(vec2 a){ return bayer2(.5 * a) * .25 + bayer2(a); }

void main(){
  vec2 frag = gl_FragCoord.xy;
  vec2 cell = floor(frag / uCell);
  vec2 local = fract(frag / uCell);
  vec2 c = (cell + .5) * uCell;
  vec2 uv = c / uRes.y;
  float t = uTime;

  // drifting field + a slow diagonal scan wave
  float n = fbm(uv * 2.4 + vec2(t * .035, -t * .025));
  n = smoothstep(.42, .9, n);
  float wave = pow(max(0., sin((uv.x * .8 + uv.y) * 2.6 - t * .5)), 18.) * .55;

  // cursor: a soft halo and a vertical stripe that flickers per row
  vec2 m = uMouse;
  float d = distance(c, m) / uRes.y;
  float halo = uHover * exp(-d * d * 30.);
  float flick = .55 + .45 * noise(vec2(cell.y * .45, t * 3.));
  float stripe = uHover * smoothstep(uCell * 1.6, 0., abs(c.x - m.x)) * flick * .7;

  float v = (n * .62 + wave) * uIntensity + halo * 1.05 + stripe;

  // intro: a radial wipe from the centre
  float r = distance(c / uRes, vec2(.5, .55));
  v *= 1. - smoothstep(uIntro * 1.1 - .18, uIntro * 1.1, r);

  // fade to the page at the bottom
  v *= smoothstep(0., .5, frag.y / uRes.y);

  // ordered dither into 5 levels
  float q = floor(v * 4. + bayer4(cell)) / 4.;
  q = clamp(q, 0., 1.);

  // pixel squares with a gap
  float g = .14;
  float sq = step(g, local.x) * step(g, local.y) * step(local.x, 1. - g) * step(local.y, 1. - g);

  vec3 bg = vec3(.0196, .047, .051);           // #050c0d
  vec3 l1 = vec3(.067, .145, .161);            // #112529
  vec3 l2 = vec3(.157, .235, .251);            // #283c40
  vec3 l3 = vec3(.082, .302, .196);            // #154d32
  vec3 l4 = vec3(.369, 1., .533);              // #5eff88
  vec3 col = q < .25 ? bg : q < .5 ? l1 : q < .75 ? l2 : q < 1. ? l3 : l4;

  // risk cells: rare, pulsing red
  float slot = floor(t * .55);
  float rk = hash(cell + slot * 1.37);
  float pulse = .5 + .5 * sin(t * 9.);
  if (rk > .99935 && v > .08) col = mix(col, vec3(1., 0., .082), pulse);

  vec3 outc = mix(bg, col, sq);
  // faint grid lines everywhere so the empty field still reads as a grid
  float line = (1. - step(.04, local.x)) + (1. - step(.04, local.y));
  outc += vec3(.02, .05, .05) * clamp(line, 0., 1.) * .6;
  gl_FragColor = vec4(outc, 1.);
}`;

onMounted(() => {
  const el = canvas.value!;
  const gl = el.getContext('webgl', { antialias: false, alpha: false, powerPreference: 'low-power' });
  if (!gl) {
    fallback.value = true;
    return;
  }
  const sh = (type: number, src: string) => {
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    return s;
  };
  const prog = gl.createProgram()!;
  gl.attachShader(prog, sh(gl.VERTEX_SHADER, VERT));
  gl.attachShader(prog, sh(gl.FRAGMENT_SHADER, FRAG));
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    fallback.value = true;
    return;
  }
  gl.useProgram(prog);
  const buf = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buf);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
  const loc = gl.getAttribLocation(prog, 'p');
  gl.enableVertexAttribArray(loc);
  gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
  const u = (n: string) => gl.getUniformLocation(prog, n);
  const uRes = u('uRes'), uTime = u('uTime'), uMouse = u('uMouse'), uHover = u('uHover');
  const uCell = u('uCell'), uIntro = u('uIntro'), uIntensity = u('uIntensity');

  const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
  const resize = () => {
    const r = el.getBoundingClientRect();
    el.width = Math.max(1, Math.round(r.width * dpr));
    el.height = Math.max(1, Math.round(r.height * dpr));
    gl.viewport(0, 0, el.width, el.height);
  };
  resize();
  const ro = new ResizeObserver(resize);
  ro.observe(el);

  const still = prefersReducedMotion();
  const mouse = { x: -1e4, y: -1e4, tx: -1e4, ty: -1e4, hover: 0, thover: 0 };
  const onMove = (e: PointerEvent) => {
    const r = el.getBoundingClientRect();
    mouse.tx = (e.clientX - r.left) * dpr;
    mouse.ty = (r.bottom - e.clientY) * dpr;
    if (mouse.x < -1e3) { mouse.x = mouse.tx; mouse.y = mouse.ty; }
    mouse.thover = e.clientY >= r.top && e.clientY <= r.bottom ? 1 : 0;
  };
  const onLeave = () => (mouse.thover = 0);
  if (props.interactive && !still) {
    window.addEventListener('pointermove', onMove, { passive: true });
    document.addEventListener('pointerleave', onLeave);
  }

  let visible = true;
  const io = new IntersectionObserver(([e]) => {
    visible = !!e?.isIntersecting;
    if (visible) loop();
  });
  io.observe(el);

  const t0 = performance.now();
  let raf = 0;
  const draw = (now: number) => {
    const t = (now - t0) / 1000;
    mouse.x += (mouse.tx - mouse.x) * 0.12;
    mouse.y += (mouse.ty - mouse.y) * 0.12;
    mouse.hover += (mouse.thover - mouse.hover) * 0.06;
    gl.uniform2f(uRes, el.width, el.height);
    gl.uniform1f(uTime, still ? 12 : t + 4);
    gl.uniform2f(uMouse, mouse.x, mouse.y);
    gl.uniform1f(uHover, mouse.hover);
    gl.uniform1f(uCell, props.cell * dpr);
    gl.uniform1f(uIntro, still ? 2 : Math.min(2, (t / 1.8) ** 1.4 * 1.2));
    gl.uniform1f(uIntensity, props.intensity);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  };
  function loop() {
    cancelAnimationFrame(raf);
    const step = (now: number) => {
      draw(now);
      if (visible && !document.hidden && !still) raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
  }
  const onVis = () => !document.hidden && visible && loop();
  document.addEventListener('visibilitychange', onVis);
  loop();

  onBeforeUnmount(() => {
    cancelAnimationFrame(raf);
    io.disconnect();
    ro.disconnect();
    window.removeEventListener('pointermove', onMove);
    document.removeEventListener('pointerleave', onLeave);
    document.removeEventListener('visibilitychange', onVis);
    gl.getExtension('WEBGL_lose_context')?.loseContext();
  });
});
</script>

<template>
  <div class="hero-canvas" :class="{ 'is-fallback': fallback }" aria-hidden="true">
    <canvas v-show="!fallback" ref="canvas" />
  </div>
</template>

<style scoped>
.hero-canvas { position: absolute; inset: 0; overflow: hidden; background: var(--dg-900); }
.hero-canvas canvas { width: 100%; height: 100%; }
.hero-canvas.is-fallback {
  background:
    radial-gradient(ellipse at 50% 40%, color-mix(in srgb, var(--bg-900) 60%, transparent), transparent 60%),
    linear-gradient(var(--dg-900), var(--dg-900)) padding-box;
}
.hero-canvas.is-fallback::before {
  content: ''; position: absolute; inset: 0;
  background-image:
    linear-gradient(to right, var(--line) 1px, transparent 1px),
    linear-gradient(to bottom, var(--line) 1px, transparent 1px);
  background-size: 13px 13px;
  mask-image: radial-gradient(ellipse at 50% 40%, #000 20%, transparent 75%);
}
</style>
