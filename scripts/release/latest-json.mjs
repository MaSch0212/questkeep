#!/usr/bin/env node
// Builds the Tauri updater manifest `latest.json` from signed bundles.
//
//   node scripts/release/latest-json.mjs --dir <assets-dir> --version <X.Y.Z> --notes <file> \
//        --repo <owner/name> --out <latest.json>
//
// Every `<bundle>.sig` next to a Windows NSIS installer (`*-setup.exe`) or a Linux `*.AppImage`
// becomes a platform entry. Exits non-zero when no signed updater bundle is found.
import fs from "node:fs";
import path from "node:path";
import { buildLatestJson, updaterPlatform } from "./lib.mjs";

function option(name) {
  const i = process.argv.indexOf(`--${name}`);
  const value = i < 0 ? undefined : process.argv[i + 1];
  if (!value) {
    console.error(`error: missing --${name}`);
    process.exit(1);
  }
  return value;
}

const dir = option("dir");
const version = option("version");
const notes = fs.readFileSync(option("notes"), "utf8").trim();
const repo = option("repo");

const assets = [];
for (const file of fs.readdirSync(dir).sort()) {
  const platform = updaterPlatform(file);
  const sig = path.join(dir, `${file}.sig`);
  if (!platform || !fs.existsSync(sig)) continue;
  assets.push({ platform, file, signature: fs.readFileSync(sig, "utf8") });
}
if (assets.length === 0) {
  console.error(`error: no signed updater bundles (*-setup.exe, *.AppImage with .sig) in ${dir}`);
  process.exit(1);
}

const manifest = buildLatestJson({
  version,
  notes,
  pubDate: new Date().toISOString(),
  repo,
  assets,
});
fs.writeFileSync(option("out"), `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Wrote latest.json for ${assets.map((a) => a.platform).join(", ")}`);
