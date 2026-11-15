#!/usr/bin/env node
// Aggregate quality gate: runs every check CI runs. Usage: `pnpm check` (or `pnpm fix` to
// auto-format and apply lint fixes first).
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const ui = path.join(root, "ui");
const fix = process.argv.includes("--fix");

/** @type {Array<[string, string, string[], string]>} label, command, args, cwd */
const steps = [
  ...(fix
    ? [
        ["cargo fmt", "cargo", ["fmt", "--all"], root],
        ["oxfmt (write)", "pnpm", ["format"], ui],
      ]
    : []),
  ["release scripts", "node", ["--test", "scripts/release/*.test.mjs"], root],
  ["rustfmt", "cargo", ["fmt", "--all", "--check"], root],
  ["clippy", "cargo", ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"], root],
  ["cargo test", "cargo", ["test", "--workspace"], root],
  // In CI, generated TS bindings must match what's committed (cargo test regenerates them).
  ...(process.env.CI
    ? [["bindings drift", "git", ["diff", "--exit-code", "--", "ui/src/bindings"], root]]
    : []),
  ["oxfmt", "pnpm", ["format:check"], ui],
  ["oxlint", "pnpm", ["lint", "--deny-warnings"], ui],
  ["tsc", "pnpm", ["typecheck"], ui],
  ["vitest", "pnpm", ["test"], ui],
];

const failed = [];
for (const [label, cmd, args, cwd] of steps) {
  console.log(`\n▶ ${label}: ${cmd} ${args.join(" ")}`);
  const started = Date.now();
  // One command string (not an args array) so Windows can resolve pnpm.cmd via the shell.
  const result = spawnSync([cmd, ...args].join(" "), { cwd, stdio: "inherit", shell: true });
  const secs = ((Date.now() - started) / 1000).toFixed(1);
  if (result.status === 0) {
    console.log(`✔ ${label} (${secs}s)`);
  } else {
    console.log(`✖ ${label} failed (${secs}s)`);
    failed.push(label);
  }
}

// Housekeeping, not a check: drop stale build output (see scripts/sweep.mjs). CI starts clean.
if (!process.env.CI) {
  console.log("\n▶ cleanup: node scripts/sweep.mjs");
  const sweep = spawnSync("node scripts/sweep.mjs", { cwd: root, stdio: "inherit", shell: true });
  if (sweep.status !== 0) console.log("⚠ cleanup of stale build output failed (not a check)");
}

if (failed.length > 0) {
  console.error(`\n${failed.length} check(s) failed: ${failed.join(", ")}`);
  process.exit(1);
}
console.log("\nAll checks passed.");
