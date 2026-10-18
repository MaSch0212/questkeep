// Pure helpers for the changeset release tooling. No I/O here, so everything is unit-tested
// (see lib.test.mjs). Only `node:` built-ins are used anywhere in scripts/release.

export const BUMPS = ["major", "minor", "patch"];

/** Parses `X.Y.Z` (no prefix, no pre-release). Returns `{ major, minor, patch }` or `null`. */
export function parseSemver(text) {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(String(text).trim());
  return m ? { major: Number(m[1]), minor: Number(m[2]), patch: Number(m[3]) } : null;
}

export function formatSemver(v) {
  return `${v.major}.${v.minor}.${v.patch}`;
}

/** Negative, zero or positive like a sort comparator. */
export function compareSemver(a, b) {
  return a.major - b.major || a.minor - b.minor || a.patch - b.patch;
}

export function bumpVersion(version, bump) {
  const v = typeof version === "string" ? parseSemver(version) : version;
  if (!v) throw new Error(`Invalid version: ${version}`);
  switch (bump) {
    case "major":
      return formatSemver({ major: v.major + 1, minor: 0, patch: 0 });
    case "minor":
      return formatSemver({ major: v.major, minor: v.minor + 1, patch: 0 });
    case "patch":
      return formatSemver({ major: v.major, minor: v.minor, patch: v.patch + 1 });
    default:
      throw new Error(`Invalid bump '${bump}' (expected ${BUMPS.join(", ")})`);
  }
}

/** The highest semver among tags named `v<int>.<int>.<int>`; `0.0.0` when there is none. */
export function currentVersionFromTags(tags) {
  let best = null;
  for (const tag of tags) {
    const m = /^v(\d+\.\d+\.\d+)$/.exec(tag.trim());
    const v = m && parseSemver(m[1]);
    if (v && (!best || compareSemver(v, best) > 0)) best = v;
  }
  return best ? formatSemver(best) : "0.0.0";
}

/**
 * Parses one changeset file: a `---` delimited header with `bump: major|minor|patch` (a trailing
 * `# comment` is allowed) followed by the release-note text.
 */
export function parseChangeset(text, name = "changeset") {
  const lines = text.replace(/^﻿/, "").split(/\r?\n/);
  if (lines[0]?.trim() !== "---") throw new Error(`${name}: must start with a '---' header`);
  const end = lines.findIndex((l, i) => i > 0 && l.trim() === "---");
  if (end < 0) throw new Error(`${name}: header is not closed with '---'`);
  const header = {};
  for (const line of lines.slice(1, end)) {
    const stripped = line.replace(/#.*$/, "").trim();
    if (!stripped) continue;
    const m = /^([A-Za-z_-]+)\s*:\s*(.*)$/.exec(stripped);
    if (!m) throw new Error(`${name}: cannot parse header line '${line}'`);
    header[m[1]] = m[2].replace(/^["']|["']$/g, "");
  }
  if (!BUMPS.includes(header.bump)) {
    throw new Error(`${name}: 'bump' must be one of ${BUMPS.join(", ")} (got '${header.bump}')`);
  }
  const body = lines
    .slice(end + 1)
    .join("\n")
    .trim();
  if (!body) throw new Error(`${name}: the description is empty`);
  return { bump: header.bump, body };
}

/** The highest bump of the changesets, or `null` for an empty list. */
export function highestBump(changesets) {
  return BUMPS.find((b) => changesets.some((c) => c.bump === b)) ?? null;
}

/** The next version, or `null` when there is nothing to release. */
export function nextVersion(current, changesets) {
  const bump = highestBump(changesets);
  return bump ? bumpVersion(current, bump) : null;
}

const HEADINGS = { major: "Major changes", minor: "Minor changes", patch: "Patch changes" };

/** Markdown release notes grouped by bump, one bullet per changeset. */
export function renderNotes(changesets) {
  const sections = [];
  for (const bump of BUMPS) {
    const items = changesets.filter((c) => c.bump === bump);
    if (items.length === 0) continue;
    const bullets = items.map((c) => {
      const [first, ...rest] = c.body.split("\n");
      return [`- ${first}`, ...rest.map((l) => (l.trim() ? `  ${l}` : ""))].join("\n");
    });
    sections.push(`### ${HEADINGS[bump]}\n\n${bullets.join("\n")}`);
  }
  return `${sections.join("\n\n")}\n`;
}

/**
 * Replaces the first `version = "..."` line inside `[section]` of a TOML text. Returns the text
 * unchanged when the section has no literal version (for example `version.workspace = true`).
 */
export function setTomlVersion(text, section, version) {
  const header = new RegExp(`^\\[${section.replace(/\./g, "\\.")}\\]\\s*$`, "m");
  const start = header.exec(text);
  if (!start) return text;
  const bodyStart = start.index + start[0].length;
  const next = text.slice(bodyStart).search(/^\[/m);
  const bodyEnd = next < 0 ? text.length : bodyStart + next;
  const body = text.slice(bodyStart, bodyEnd);
  const replaced = body.replace(/^(version\s*=\s*)"[^"]*"/m, `$1"${version}"`);
  return text.slice(0, bodyStart) + replaced + text.slice(bodyEnd);
}

/** Replaces the top-level `"version"` string of a JSON text without reformatting the file. */
export function setJsonVersion(text, version) {
  if (!/^ {0,4}"version"\s*:\s*"[^"]*"/m.test(text)) throw new Error("no top-level version found");
  return text.replace(/^( {0,4}"version"\s*:\s*)"[^"]*"/m, `$1"${version}"`);
}

/** Sets the version of the named `[[package]]` entries in a Cargo.lock text. */
export function setCargoLockVersions(text, names, version) {
  return text.replace(
    /(\[\[package\]\]\r?\nname = "([^"]+)"\r?\nversion = )"[^"]*"/g,
    (whole, head, name) => (names.includes(name) ? `${head}"${version}"` : whole),
  );
}

/**
 * Builds the Tauri updater manifest. `assets` are `{ platform, file, signature }` where the
 * signature is the content of the file's `.sig`.
 */
export function buildLatestJson({ version, notes, pubDate, repo, assets }) {
  const platforms = {};
  for (const a of assets) {
    platforms[a.platform] = {
      signature: a.signature.trim(),
      url: `https://github.com/${repo}/releases/download/v${version}/${a.file}`,
    };
  }
  return { version, notes, pub_date: pubDate, platforms };
}

/** Maps a built file name to its updater platform key, or `null` if it is not an updater bundle. */
export function updaterPlatform(file) {
  if (/-setup\.exe$/i.test(file)) return "windows-x86_64";
  if (/\.AppImage$/i.test(file)) return "linux-x86_64";
  return null;
}
