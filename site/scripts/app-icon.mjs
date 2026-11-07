// Renders the Iso cube logo (design/README.md) as assets/winsploit.ico, the
// icon build.rs embeds in winsploit.exe (Explorer, Task Manager, taskbar).
//   node site/scripts/app-icon.mjs
// No dependencies: the faces are polygons and the prompt is two round-capped
// strokes, so a small supersampling rasterizer draws them exactly; each size
// is stored as a PNG inside the .ico (Windows Vista and later read those).
import { writeFileSync, mkdirSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const SIZES = [16, 20, 24, 32, 40, 48, 64, 256];

// fixed Frost colours, as in the favicon
const LEFT = [0x46, 0xa0, 0xb9];
const TOP = [0xbc, 0xdd, 0xe6];
const RIGHT = [0x89, 0xc2, 0xd2];
const INK = [0x0c, 0x19, 0x1d];
const HEX = [[16, 2.5], [28, 9.3], [28, 22.7], [16, 29.5], [4, 22.7], [4, 9.3]];
const TOP_FACE = [[16, 2.5], [28, 9.3], [16, 16], [4, 9.3]];
const RIGHT_FACE = [[16, 16], [28, 9.3], [28, 22.7], [16, 29.5]];
// the prompt `>` and `_`, stroke width 1.7 with round caps and joins
const STROKES = [[[10.5, 8], [13.5, 9.6], [10.5, 11.3]], [[15, 11.5], [18.2, 13.3]]];
const HALF = 1.7 / 2;
// the drawing spans x 4..28, y 2.5..29.5; frame it in a 28-unit square
const VIEW = { x: 2, y: 2, size: 28 };

const inside = (poly, x, y) => {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i];
    const [xj, yj] = poly[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) c = !c;
  }
  return c;
};
const segDist = (x, y, [ax, ay], [bx, by]) => {
  const dx = bx - ax;
  const dy = by - ay;
  const t = Math.max(0, Math.min(1, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)));
  return Math.hypot(x - (ax + t * dx), y - (ay + t * dy));
};
const onStroke = (x, y) =>
  STROKES.some((pts) => pts.slice(1).some((p, i) => segDist(x, y, pts[i], p) <= HALF));

/** The colour at a point of the 32-unit grid, or null outside the cube. */
function sample(x, y) {
  if (!inside(HEX, x, y)) return null;
  if (onStroke(x, y)) return INK;
  if (inside(RIGHT_FACE, x, y)) return RIGHT;
  if (inside(TOP_FACE, x, y)) return TOP;
  return LEFT;
}

/** RGBA pixels for an n x n icon, 8 x 8 samples per pixel. */
function render(n) {
  const SS = 8;
  const px = Buffer.alloc(n * n * 4);
  const unit = VIEW.size / n;
  for (let py = 0; py < n; py++) {
    for (let pxl = 0; pxl < n; pxl++) {
      let r = 0, g = 0, b = 0, a = 0;
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const c = sample(VIEW.x + (pxl + (sx + 0.5) / SS) * unit, VIEW.y + (py + (sy + 0.5) / SS) * unit);
          if (!c) continue;
          r += c[0]; g += c[1]; b += c[2]; a++;
        }
      }
      const o = (py * n + pxl) * 4;
      if (a) {
        px[o] = Math.round(r / a);
        px[o + 1] = Math.round(g / a);
        px[o + 2] = Math.round(b / a);
        px[o + 3] = Math.round((a / (SS * SS)) * 255);
      }
    }
  }
  return px;
}

const CRC = new Int32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c;
});
const crc32 = (buf) => {
  let c = -1;
  for (const byte of buf) c = CRC[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
};

function png(n, rgba) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(n, 0);
  ihdr.writeUInt32BE(n, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  const raw = Buffer.alloc(n * (n * 4 + 1));
  for (let y = 0; y < n; y++) rgba.copy(raw, y * (n * 4 + 1) + 1, y * n * 4, (y + 1) * n * 4);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

const images = SIZES.map((n) => png(n, render(n)));
const header = Buffer.alloc(6);
header.writeUInt16LE(0, 0);
header.writeUInt16LE(1, 2); // icon
header.writeUInt16LE(images.length, 4);
let offset = 6 + 16 * images.length;
const dir = images.map((img, i) => {
  const e = Buffer.alloc(16);
  const n = SIZES[i];
  e[0] = n >= 256 ? 0 : n;
  e[1] = n >= 256 ? 0 : n;
  e.writeUInt16LE(1, 4); // planes
  e.writeUInt16LE(32, 6); // bits per pixel
  e.writeUInt32LE(img.length, 8);
  e.writeUInt32LE(offset, 12);
  offset += img.length;
  return e;
});

const out = join(repo, 'assets/winsploit.ico');
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, Buffer.concat([header, ...dir, ...images]));
if (process.argv.includes('--png')) {
  for (const [i, n] of SIZES.entries()) writeFileSync(join(dirname(out), `icon-${n}.png`), images[i]);
}
console.log(`app icon: ${SIZES.join(', ')} px -> assets/winsploit.ico`);
