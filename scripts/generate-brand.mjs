#!/usr/bin/env node
// Renders the installer artwork from the app icon in src-tauri/icons/source.png, so the shortcut,
// taskbar and installer always match. Run via `npm run brand`, which also regenerates the icon set.
import { Resvg } from "@resvg/resvg-js";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const icon = readFileSync(join(root, "src-tauri/icons/source.png")).toString("base64");
const installerDir = join(root, "src-tauri/installer");
mkdirSync(installerDir, { recursive: true });

// Installer art is the icon placed on the app's navy gradient background.
function banner(width, height, size, x, y) {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${width}" height="${height}">
    <defs><linearGradient id="page" x1="0" y1="0" x2="1" y2="1">
      <stop stop-color="#141e44"/><stop offset="1" stop-color="#090e20"/>
    </linearGradient></defs>
    <rect width="${width}" height="${height}" fill="url(#page)"/>
    <image x="${x}" y="${y}" width="${size}" height="${size}" xlink:href="data:image/png;base64,${icon}"/></svg>`;
  const image = new Resvg(svg).render();
  return { width: image.width, height: image.height, rgba: image.pixels };
}

// Installers need 24-bit bottom-up BMPs, which resvg does not produce.
function toBmp({ width, height, rgba }) {
  const rowSize = Math.ceil((width * 3) / 4) * 4;
  const size = 54 + rowSize * height;
  const b = Buffer.alloc(size);
  b.write("BM", 0);
  b.writeUInt32LE(size, 2);
  b.writeUInt32LE(54, 10);
  b.writeUInt32LE(40, 14);
  b.writeInt32LE(width, 18);
  b.writeInt32LE(height, 22);
  b.writeUInt16LE(1, 26);
  b.writeUInt16LE(24, 28);
  b.writeUInt32LE(rowSize * height, 34);
  for (let y = 0; y < height; y++) {
    const row = 54 + (height - 1 - y) * rowSize;
    for (let x = 0; x < width; x++) {
      const p = (y * width + x) * 4;
      b[row + x * 3] = rgba[p + 2];
      b[row + x * 3 + 1] = rgba[p + 1];
      b[row + x * 3 + 2] = rgba[p];
    }
  }
  return b;
}

const art = {
  "nsis-header.bmp": banner(150, 57, 49, 97, 4),
  "nsis-sidebar.bmp": banner(164, 314, 136, 14, 89),
  "wix-banner.bmp": banner(493, 58, 50, 439, 4),
  "wix-dialog.bmp": banner(493, 312, 130, 14, 91),
};
for (const [name, image] of Object.entries(art)) {
  writeFileSync(join(installerDir, name), toBmp(image));
}
console.log("Installer art generated.");
