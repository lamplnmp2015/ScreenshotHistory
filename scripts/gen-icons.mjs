// Generates app icons for Tauri without any external image tooling.
// Renders a simple "screenshot + history clock" mark via supersampled SDF
// drawing, then writes the PNG sizes and a multi-size .ico that Tauri needs.
//
//   node scripts/gen-icons.mjs
//
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const OUT = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "icons");
mkdirSync(OUT, { recursive: true });

// --- palette ----------------------------------------------------------------
const TOP = [92, 124, 250]; // #5c7cfa
const BOT = [66, 99, 235]; // #4263eb
const WHITE = [255, 255, 255];
const ACCENT = [56, 84, 200];
const TRANSPARENT = [0, 0, 0, 0];

// --- geometry helpers (normalized 0..1 space) -------------------------------
function roundRectSdf(x, y, x0, y0, x1, y1, r) {
  const hx = (x1 - x0) / 2;
  const hy = (y1 - y0) / 2;
  const cx = (x0 + x1) / 2;
  const cy = (y0 + y1) / 2;
  const qx = Math.abs(x - cx) - (hx - r);
  const qy = Math.abs(y - cy) - (hy - r);
  return Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - r;
}
function dist(ax, ay, bx, by) {
  return Math.hypot(ax - bx, ay - by);
}
function inTriangle(px, py, ax, ay, bx, by, cx, cy) {
  const d1 = (px - bx) * (ay - by) - (ax - bx) * (py - by);
  const d2 = (px - cx) * (by - cy) - (bx - cx) * (py - cy);
  const d3 = (px - ax) * (cy - ay) - (cx - ax) * (py - ay);
  const neg = d1 < 0 || d2 < 0 || d3 < 0;
  const pos = d1 > 0 || d2 > 0 || d3 > 0;
  return !(neg && pos);
}
function nearSegment(px, py, ax, ay, bx, by, halfw) {
  const vx = bx - ax;
  const vy = by - ay;
  const len2 = vx * vx + vy * vy || 1e-9;
  let t = ((px - ax) * vx + (py - ay) * vy) / len2;
  t = Math.max(0, Math.min(1, t));
  return dist(px, py, ax + t * vx, ay + t * vy) <= halfw;
}

// Topmost-first color lookup at a normalized point. Returns [r,g,b,a].
function colorAt(x, y) {
  // clock badge
  const ccx = 0.7,
    ccy = 0.72,
    cr = 0.215,
    faceR = 0.16;
  const dc = dist(x, y, ccx, ccy);
  if (dc <= cr) {
    if (dc > faceR) return [...ACCENT, 255]; // ring
    // hands: up + right
    if (
      nearSegment(x, y, ccx, ccy, ccx, ccy - 0.1, 0.013) ||
      nearSegment(x, y, ccx, ccy, ccx + 0.085, ccy, 0.013)
    )
      return [...ACCENT, 255];
    return [...WHITE, 255];
  }

  // photo frame
  if (roundRectSdf(x, y, 0.18, 0.17, 0.72, 0.59, 0.05) <= 0) {
    if (dist(x, y, 0.33, 0.29) <= 0.05) return [...ACCENT, 255]; // sun
    if (inTriangle(x, y, 0.24, 0.55, 0.44, 0.33, 0.66, 0.55)) return [...ACCENT, 255]; // mountain
    return [...WHITE, 255];
  }

  // background rounded square (vertical gradient)
  if (roundRectSdf(x, y, 0.04, 0.04, 0.96, 0.96, 0.22) <= 0) {
    const t = y;
    return [
      Math.round(TOP[0] + (BOT[0] - TOP[0]) * t),
      Math.round(TOP[1] + (BOT[1] - TOP[1]) * t),
      Math.round(TOP[2] + (BOT[2] - TOP[2]) * t),
      255,
    ];
  }
  return TRANSPARENT;
}

// Render an RGBA buffer at the given pixel size with 4x4 supersampling (AA).
function render(size) {
  const SS = 4;
  const buf = Buffer.alloc(size * size * 4);
  for (let py = 0; py < size; py++) {
    for (let px = 0; px < size; px++) {
      let r = 0,
        g = 0,
        b = 0,
        a = 0;
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const nx = (px + (sx + 0.5) / SS) / size;
          const ny = (py + (sy + 0.5) / SS) / size;
          const c = colorAt(nx, ny);
          const ca = c[3] / 255;
          r += c[0] * ca;
          g += c[1] * ca;
          b += c[2] * ca;
          a += ca;
        }
      }
      const n = SS * SS;
      const o = (py * size + px) * 4;
      // premultiplied-average -> straight color
      const al = a / n;
      buf[o] = al > 0 ? Math.round(r / a) : 0;
      buf[o + 1] = al > 0 ? Math.round(g / a) : 0;
      buf[o + 2] = al > 0 ? Math.round(b / a) : 0;
      buf[o + 3] = Math.round(al * 255);
    }
  }
  return buf;
}

// --- PNG encoder (RGBA, 8-bit) ----------------------------------------------
function crc32(buf) {
  let c = ~0;
  for (let i = 0; i < buf.length; i++) {
    c ^= buf[i];
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
  }
  return (~c) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const body = Buffer.concat([typeBuf, data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([len, body, crc]);
}
function encodePng(rgba, size) {
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type RGBA
  // filter byte 0 per scanline
  const stride = size * 4;
  const raw = Buffer.alloc((stride + 1) * size);
  for (let y = 0; y < size; y++) {
    raw[y * (stride + 1)] = 0;
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, y * stride + stride);
  }
  const idat = deflateSync(raw, { level: 9 });
  return Buffer.concat([
    sig,
    chunk("IHDR", ihdr),
    chunk("IDAT", idat),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// --- ICO (embeds PNG entries; valid for Vista+ and Tauri's ico reader) ------
function encodeIco(entries) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type = icon
  header.writeUInt16LE(entries.length, 4);

  const dir = Buffer.alloc(16 * entries.length);
  let offset = 6 + dir.length;
  const datas = [];
  entries.forEach((e, i) => {
    const o = i * 16;
    dir[o] = e.size >= 256 ? 0 : e.size;
    dir[o + 1] = e.size >= 256 ? 0 : e.size;
    dir[o + 2] = 0; // palette
    dir[o + 3] = 0; // reserved
    dir.writeUInt16LE(1, o + 4); // planes
    dir.writeUInt16LE(32, o + 6); // bpp
    dir.writeUInt32LE(e.png.length, o + 8);
    dir.writeUInt32LE(offset, o + 12);
    offset += e.png.length;
    datas.push(e.png);
  });
  return Buffer.concat([header, dir, ...datas]);
}

// --- emit --------------------------------------------------------------------
const pngTargets = {
  "32x32.png": 32,
  "128x128.png": 128,
  "128x128@2x.png": 256,
  "icon.png": 512,
  "Square30x30Logo.png": 30,
  "Square44x44Logo.png": 44,
  "Square71x71Logo.png": 71,
  "Square89x89Logo.png": 89,
  "Square107x107Logo.png": 107,
  "Square142x142Logo.png": 142,
  "Square150x150Logo.png": 150,
  "Square284x284Logo.png": 284,
  "Square310x310Logo.png": 310,
  "StoreLogo.png": 50,
};

const cache = new Map();
function pngFor(size) {
  if (!cache.has(size)) cache.set(size, encodePng(render(size), size));
  return cache.get(size);
}

for (const [name, size] of Object.entries(pngTargets)) {
  writeFileSync(join(OUT, name), pngFor(size));
  console.log("wrote", name, `${size}x${size}`);
}

const icoSizes = [16, 24, 32, 48, 64, 128, 256];
const ico = encodeIco(icoSizes.map((s) => ({ size: s, png: pngFor(s) })));
writeFileSync(join(OUT, "icon.ico"), ico);
console.log("wrote icon.ico", icoSizes.join(","));

console.log("done.");
