// Renders the ASCII wordmark (banner.rs / README) as .github/banner.svg, so
// the README can show it centered and crisp instead of in a code block.
//   node site/scripts/readme-banner.mjs
// █ cells become filled rects (merged per row run) with the site's green
// gradient; the ╔═╗║╚╝ shadow becomes thin double strokes.
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

const ART = [
  ' ██╗    ██╗██╗███╗   ██╗███████╗██████╗ ██╗      ██████╗ ██╗████████╗',
  ' ██║    ██║██║████╗  ██║██╔════╝██╔══██╗██║     ██╔═══██╗██║╚══██╔══╝',
  ' ██║ █╗ ██║██║██╔██╗ ██║███████╗██████╔╝██║     ██║   ██║██║   ██║',
  ' ██║███╗██║██║██║╚██╗██║╚════██║██╔═══╝ ██║     ██║   ██║██║   ██║',
  ' ╚███╔███╔╝██║██║ ╚████║███████║██║     ███████╗╚██████╔╝██║   ██║',
  '  ╚══╝╚══╝ ╚═╝╚═╝  ╚═══╝╚══════╝╚═╝     ╚══════╝ ╚═════╝ ╚═╝   ╚═╝',
].map((l) => [...l]);

const CW = 12; // cell width
const CH = 22; // cell height
const D = 2.2; // half gap between the two strokes of a double line
const cols = Math.max(...ART.map((l) => l.length));
// trim the common leading blank column so the art is optically centered
const lead = Math.min(...ART.map((l) => l.findIndex((c) => c !== ' ')));
const artW = (cols - lead) * CW;
const artH = ART.length * CH;
const PX = 44;
const PY = 34;
const W = artW + PX * 2;
const H = artH + PY * 2;

// double-line paths per box-drawing character, in cell coordinates
const cx = CW / 2;
const cy = CH / 2;
const P = {
  '═': [[[0, cy - D], [CW, cy - D]], [[0, cy + D], [CW, cy + D]]],
  '║': [[[cx - D, 0], [cx - D, CH]], [[cx + D, 0], [cx + D, CH]]],
  '╔': [[[cx - D, CH], [cx - D, cy - D], [CW, cy - D]], [[cx + D, CH], [cx + D, cy + D], [CW, cy + D]]],
  '╗': [[[0, cy - D], [cx + D, cy - D], [cx + D, CH]], [[0, cy + D], [cx - D, cy + D], [cx - D, CH]]],
  '╚': [[[cx - D, 0], [cx - D, cy + D], [CW, cy + D]], [[cx + D, 0], [cx + D, cy - D], [CW, cy - D]]],
  '╝': [[[cx + D, 0], [cx + D, cy + D], [0, cy + D]], [[cx - D, 0], [cx - D, cy - D], [0, cy - D]]],
};

const r = (n) => Math.round(n * 100) / 100;
const rects = [];
const paths = [];
ART.forEach((line, row) => {
  const y = PY + row * CH;
  for (let i = lead; i < line.length; ) {
    const ch = line[i];
    const x = PX + (i - lead) * CW;
    if (ch === '█') {
      // all blocks go into ONE path, so the renderer anti-aliases the
      // letters as a whole and no seams show between neighbouring cells
      let j = i;
      while (line[j] === '█') j++;
      rects.push(`M${x} ${y}h${(j - i) * CW}v${CH}h-${(j - i) * CW}z`);
      i = j;
      continue;
    }
    if (P[ch]) {
      for (const pts of P[ch]) paths.push('M' + pts.map(([px, py]) => `${r(x + px)} ${r(y + py)}`).join('L'));
    } else if (ch !== ' ') {
      throw new Error(`banner: no shape for ${JSON.stringify(ch)}`);
    }
    i++;
  }
});

const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}" role="img" aria-label="Winsploit">
  <title>Winsploit</title>
  <defs>
    <linearGradient id="g" x1="0" y1="${PY}" x2="0" y2="${PY + artH}" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#c8ffd9"/>
      <stop offset="0.45" stop-color="#5eff88"/>
      <stop offset="1" stop-color="#2a9b5c"/>
    </linearGradient>
    <radialGradient id="glow" cx="0.5" cy="0.5" r="0.6">
      <stop offset="0" stop-color="#5eff88" stop-opacity="0.10"/>
      <stop offset="1" stop-color="#5eff88" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="${W}" height="${H}" rx="14" fill="#050c0d"/>
  <rect width="${W}" height="${H}" rx="14" fill="url(#glow)"/>
  <rect x="0.5" y="0.5" width="${W - 1}" height="${H - 1}" rx="13.5" fill="none" stroke="#1b2f33"/>
  <path d="${paths.join('')}" fill="none" stroke="#3a6063" stroke-width="1.3" stroke-linejoin="miter"/>
  <path fill="url(#g)" d="${rects.join('')}"/>
</svg>
`;

const out = join(repo, '.github/banner.svg');
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, svg);
console.log(`banner: ${W}×${H}, ${rects.length} block runs, ${paths.length} strokes -> .github/banner.svg`);
