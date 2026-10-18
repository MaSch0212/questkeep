#!/usr/bin/env node
// Builds assets/icon.svg (logo in the accent color on a dark rounded tile) from assets/logo.svg.
// Regenerate the platform icons afterwards with: pnpm tauri icon assets/icon.svg -o src-tauri/icons
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const src = fs.readFileSync(path.join(root, "assets/logo.svg"), "utf8");
const [minX, minY, width, height] = /viewBox="([^"]+)"/.exec(src)[1].split(/\s+/).map(Number);
const paths = [...src.matchAll(/<path d="([^"]+)"\s*\/>/g)].map((m) => m[1]);

const size = 512;
const scale = (size * 0.66) / Math.max(width, height);
const tx = (size - width * scale) / 2 - minX * scale;
const ty = (size - height * scale) / 2 - minY * scale;

const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}">
  <rect width="${size}" height="${size}" rx="112" fill="#1b1f24"/>
  <g fill="#7c9cff" transform="translate(${tx.toFixed(2)} ${ty.toFixed(2)}) scale(${scale.toFixed(5)})">
${paths.map((d) => `    <path d="${d}"/>`).join("\n")}
  </g>
</svg>
`;
fs.writeFileSync(path.join(root, "assets/icon.svg"), svg);
console.log("wrote assets/icon.svg");
