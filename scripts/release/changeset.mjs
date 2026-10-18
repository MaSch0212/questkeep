#!/usr/bin/env node
// Changeset CLI. Dependency-free (Node 24, `node:` built-ins only).
//
//   node scripts/release/changeset.mjs add <major|minor|patch> "<summary>"
//   node scripts/release/changeset.mjs status
//   node scripts/release/changeset.mjs plan --out-dir <dir> [--github-output]
//   node scripts/release/changeset.mjs version --write <X.Y.Z>
//   node scripts/release/changeset.mjs notes --out <file>
//   node scripts/release/changeset.mjs consume [--list <file>]
//
// The current version is the highest `vX.Y.Z` git tag (0.0.0 without tags). See docs/RELEASING.md.
import { execFileSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  BUMPS,
  currentVersionFromTags,
  nextVersion,
  parseChangeset,
  parseSemver,
  renderNotes,
  setCargoLockVersions,
  setJsonVersion,
  setTomlVersion,
} from "./lib.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const changesetDir = path.join(root, ".changeset");

function fail(message) {
  console.error(`error: ${message}`);
  process.exit(1);
}

/** Value following `--name`, or `undefined`. */
function option(args, name) {
  const i = args.indexOf(`--${name}`);
  if (i < 0) return undefined;
  const value = args[i + 1];
  if (value === undefined || value.startsWith("--")) fail(`--${name} needs a value`);
  return value;
}

function currentVersion() {
  const out = execFileSync("git", ["tag", "--list", "v*"], { cwd: root, encoding: "utf8" });
  return currentVersionFromTags(out.split(/\r?\n/).filter(Boolean));
}

/** Pending changesets as `{ file, bump, body }`, sorted by file name. Throws on malformed files. */
function pendingChangesets() {
  if (!fs.existsSync(changesetDir)) return [];
  return fs
    .readdirSync(changesetDir)
    .filter((f) => f.endsWith(".md") && f.toLowerCase() !== "readme.md")
    .sort()
    .map((file) => {
      const text = fs.readFileSync(path.join(changesetDir, file), "utf8");
      return { file, ...parseChangeset(text, `.changeset/${file}`) };
    });
}

function readRel(rel) {
  return fs.readFileSync(path.join(root, rel), "utf8");
}

function writeRel(rel, text) {
  fs.writeFileSync(path.join(root, rel), text);
}

function writeVersion(version) {
  if (!parseSemver(version)) fail(`'${version}' is not a valid X.Y.Z version`);
  const edits = [
    ["src-tauri/tauri.conf.json", (t) => setJsonVersion(t, version)],
    ["ui/package.json", (t) => setJsonVersion(t, version)],
    // Both crates inherit `version.workspace = true`, so the real value lives in the root manifest.
    ["Cargo.toml", (t) => setTomlVersion(t, "workspace.package", version)],
    // A literal version in a crate manifest is updated too; inherited ones are left alone.
    ["src-tauri/Cargo.toml", (t) => setTomlVersion(t, "package", version)],
    ["crates/questkeep-core/Cargo.toml", (t) => setTomlVersion(t, "package", version)],
    ["Cargo.lock", (t) => setCargoLockVersions(t, ["questkeep", "questkeep-core"], version)],
  ];
  for (const [rel, edit] of edits) {
    const before = readRel(rel);
    const after = edit(before);
    if (after !== before) writeRel(rel, after);
    console.log(`${after === before ? "unchanged" : "updated"}  ${rel}`);
  }
}

function printStatus() {
  const current = currentVersion();
  const changesets = pendingChangesets();
  const next = nextVersion(current, changesets);
  console.log(`Current version (latest v* tag): ${current}`);
  console.log(`Pending changesets: ${changesets.length}`);
  for (const c of changesets) console.log(`  [${c.bump}] ${c.file}: ${c.body.split("\n")[0]}`);
  console.log(next ? `Next version: ${next}` : "Next version: none (nothing to release)");
}

function appendGithubOutput(values) {
  const file = process.env.GITHUB_OUTPUT;
  if (!file) fail("GITHUB_OUTPUT is not set");
  fs.appendFileSync(file, Object.entries(values).map(([k, v]) => `${k}=${v}\n`).join(""));
}

const [command, ...args] = process.argv.slice(2);

switch (command) {
  case "add": {
    const [bump, ...summary] = args;
    if (!BUMPS.includes(bump)) fail(`usage: add <${BUMPS.join("|")}> "<summary>"`);
    const body = summary.join(" ").trim();
    if (!body) fail("the summary must not be empty");
    fs.mkdirSync(changesetDir, { recursive: true });
    const file = `${randomBytes(4).toString("hex")}.md`;
    fs.writeFileSync(path.join(changesetDir, file), `---\nbump: ${bump}\n---\n${body}\n`);
    console.log(`Created .changeset/${file}`);
    break;
  }
  case "status":
    printStatus();
    break;
  case "plan": {
    // Used by the release workflow: decides whether there is a release and records what it contains.
    const outDir = option(args, "out-dir") ?? fail("plan needs --out-dir <dir>");
    const current = currentVersion();
    const changesets = pendingChangesets();
    const next = nextVersion(current, changesets);
    fs.mkdirSync(outDir, { recursive: true });
    if (next) {
      fs.writeFileSync(path.join(outDir, "notes.md"), renderNotes(changesets));
      fs.writeFileSync(path.join(outDir, "consumed.txt"), `${changesets.map((c) => c.file).join("\n")}\n`);
    }
    printStatus();
    if (args.includes("--github-output")) {
      appendGithubOutput({ has_release: next ? "true" : "false", current, version: next ?? "" });
    }
    break;
  }
  case "version": {
    const version = option(args, "write") ?? fail("usage: version --write <X.Y.Z>");
    writeVersion(version);
    break;
  }
  case "notes": {
    const out = option(args, "out") ?? fail("usage: notes --out <file>");
    const changesets = pendingChangesets();
    if (changesets.length === 0) fail("no pending changesets");
    fs.writeFileSync(out, renderNotes(changesets));
    console.log(`Wrote release notes for ${changesets.length} changeset(s) to ${out}`);
    break;
  }
  case "consume": {
    // Deletes the changeset files of a release: those named in --list, or all pending ones.
    const list = option(args, "list");
    const names = list
      ? fs.readFileSync(list, "utf8").split(/\r?\n/).filter(Boolean)
      : pendingChangesets().map((c) => c.file);
    for (const name of names) {
      if (name !== path.basename(name) || !name.endsWith(".md") || name.toLowerCase() === "readme.md") {
        fail(`refusing to delete '${name}'`);
      }
      fs.rmSync(path.join(changesetDir, name), { force: true });
      console.log(`deleted .changeset/${name}`);
    }
    break;
  }
  default:
    fail("usage: changeset.mjs <add|status|plan|version|notes|consume> ...");
}
