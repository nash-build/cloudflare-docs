'use strict';
// Generates the icon assets with zero dependencies (pure Node + zlib):
//   trayTemplate.png      16x16  monochrome (macOS menu-bar template image)
//   trayTemplate@2x.png   32x32  monochrome
//   icon.png              512x512 colored app icon (electron-builder -> .icns)
// Run: node assets/make-icons.js
const zlib = require('zlib');
const fs = require('fs');
const path = require('path');

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
  len.writeUInt32BE(data.length);
  const typeBuf = Buffer.from(type, 'ascii');
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])));
  return Buffer.concat([len, typeBuf, data, crc]);
}

function encodePNG(width, height, rgba) {
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (stride + 1)] = 0; // filter: none
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, y * stride + stride);
  }
  const idat = zlib.deflateSync(raw, { level: 9 });
  return Buffer.concat([sig, chunk('IHDR', ihdr), chunk('IDAT', idat), chunk('IEND', Buffer.alloc(0))]);
}

// distance from point p to segment a-b (all normalized 0..1)
function distToSeg(px, py, ax, ay, bx, by) {
  const dx = bx - ax, dy = by - ay;
  const len2 = dx * dx + dy * dy || 1e-9;
  let t = ((px - ax) * dx + (py - ay) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  const cx = ax + t * dx, cy = ay + t * dy;
  return Math.hypot(px - cx, py - cy);
}

// Anti-aliased coverage of a checkmark at normalized coords.
function checkCoverage(nx, ny, thickness) {
  const d = Math.min(
    distToSeg(nx, ny, 0.24, 0.55, 0.43, 0.73),
    distToSeg(nx, ny, 0.43, 0.73, 0.78, 0.30)
  );
  const edge = 0.012;
  return Math.max(0, Math.min(1, (thickness / 2 - d) / edge + 0.5));
}

function rounded(nx, ny, r) {
  // signed coverage for a rounded square covering the canvas with corner r
  const x = Math.abs(nx - 0.5), y = Math.abs(ny - 0.5);
  const half = 0.5;
  const qx = Math.max(x - (half - r), 0);
  const qy = Math.max(y - (half - r), 0);
  const d = Math.hypot(qx, qy) - r;
  return Math.max(0, Math.min(1, (-(d) ) / 0.01 + 0.5));
}

function makeTray(size) {
  const rgba = Buffer.alloc(size * size * 4);
  const thickness = 0.16;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const nx = (x + 0.5) / size, ny = (y + 0.5) / size;
      const a = checkCoverage(nx, ny, thickness);
      const i = (y * size + x) * 4;
      rgba[i] = 0; rgba[i + 1] = 0; rgba[i + 2] = 0; // black; macOS recolors template
      rgba[i + 3] = Math.round(a * 255);
    }
  }
  return encodePNG(size, size, rgba);
}

function makeAppIcon(size) {
  const rgba = Buffer.alloc(size * size * 4);
  const thickness = 0.14;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const nx = (x + 0.5) / size, ny = (y + 0.5) / size;
      const bg = rounded(nx, ny, 0.22);
      // vertical accent gradient
      const top = [124, 156, 255], bot = [90, 116, 220];
      const t = ny;
      const r = Math.round(top[0] * (1 - t) + bot[0] * t);
      const g = Math.round(top[1] * (1 - t) + bot[1] * t);
      const b = Math.round(top[2] * (1 - t) + bot[2] * t);
      const check = checkCoverage(nx, ny, thickness);
      const i = (y * size + x) * 4;
      // white check over accent background
      rgba[i] = Math.round(r * (1 - check) + 255 * check);
      rgba[i + 1] = Math.round(g * (1 - check) + 255 * check);
      rgba[i + 2] = Math.round(b * (1 - check) + 255 * check);
      rgba[i + 3] = Math.round(bg * 255);
    }
  }
  return encodePNG(size, size, rgba);
}

const dir = __dirname;
fs.writeFileSync(path.join(dir, 'trayTemplate.png'), makeTray(16));
fs.writeFileSync(path.join(dir, 'trayTemplate@2x.png'), makeTray(32));
fs.writeFileSync(path.join(dir, 'icon.png'), makeAppIcon(512));
console.log('Wrote trayTemplate.png, trayTemplate@2x.png, icon.png');
