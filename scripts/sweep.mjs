#!/usr/bin/env node
// Removes stale build output from the cargo target folders with cargo-sweep
// (`cargo install cargo-sweep`): artifacts of toolchains that are no longer installed, and
// artifacts no build used in the last DAYS days. Cargo itself never cleans target/, which grew to
// hundreds of GB. Usage: `pnpm clean:stale`; `pnpm check` runs it at the end too.
// Skips quietly (exit 0) when cargo-sweep is missing.
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const DAYS = 7;
const TIMEOUT_MS = 10 * 60 * 1000;

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function run(args, cwd, env = {}) {
  // One command string (not an args array) so Windows resolves cargo the same way check.mjs does.
  return spawnSync(["cargo", ...args].join(" "), {
    cwd,
    stdio: "inherit",
    shell: true,
    timeout: TIMEOUT_MS,
    env: { ...process.env, ...env },
  });
}

const probe = spawnSync("cargo sweep --version", { shell: true, stdio: "ignore", timeout: 30_000 });
if (probe.status !== 0) {
  console.log("cargo-sweep is not installed; skipping the cleanup (cargo install cargo-sweep).");
  process.exit(0);
}

/** Each entry: label, project dir, target dir override (null = the project's default). */
const targets = [
  ["target", root, null],
  // The hidden agent test instance builds into its own folder.
  ["target/agent", root, path.join(root, "target", "agent")],
  ["example-store", path.join(root, "examples", "plugins", "example-store"), null],
];

let failed = false;
for (const [label, cwd, targetDir] of targets) {
  const dir = targetDir ?? path.join(cwd, "target");
  if (!existsSync(dir)) continue;
  console.log(`\n▶ sweep ${label}`);
  const env = targetDir ? { CARGO_TARGET_DIR: targetDir } : {};
  for (const args of [
    ["sweep", "--installed"],
    ["sweep", "--time", String(DAYS)],
  ]) {
    const result = run(args, cwd, env);
    if (result.status !== 0) {
      console.log(`✖ cargo ${args.join(" ")} failed for ${label}`);
      failed = true;
    }
  }
}
process.exit(failed ? 1 : 0);
