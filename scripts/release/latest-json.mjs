#!/usr/bin/env node
// Builds the Tauri updater manifest `latest.json` from signed bundles.
//
//   node scripts/release/latest-json.mjs --dir <assets-dir> --version <X.Y.Z> --notes <file> \
//        --repo <owner/name> --out <latest.json>
//
// Every `<bundle>.sig` next to an installer or package (NSIS, MSI, AppImage, deb, rpm) becomes
// platform entries (see `updaterPlatforms`); the portable exe has none. Exits non-zero when no
// signed updater bundle is found.
import fs from "node:fs";
import path from "node:path";
import { buildLatestJson, updaterPlatforms } from "./lib.mjs";

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
  const sig = path.join(dir, `${file}.sig`);
  if (!fs.existsSync(sig)) continue;
  for (const platform of updaterPlatforms(file)) {
    assets.push({ platform, file, signature: fs.readFileSync(sig, "utf8") });
  }
}
if (assets.length === 0) {
  console.error(`error: no signed updater bundles (installers or packages with a .sig) in ${dir}`);
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
