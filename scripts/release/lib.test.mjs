import assert from "node:assert/strict";
import { test } from "node:test";
import {
  buildLatestJson,
  bumpVersion,
  compareSemver,
  currentVersionFromTags,
  highestBump,
  nextVersion,
  parseChangeset,
  parseSemver,
  renderNotes,
  setCargoLockVersions,
  setJsonVersion,
  setTomlVersion,
  updaterPlatform,
} from "./lib.mjs";

test("parseSemver is strict", () => {
  assert.deepEqual(parseSemver("1.22.3"), { major: 1, minor: 22, patch: 3 });
  for (const bad of ["1.2", "v1.2.3", "1.2.3-rc.1", "a.b.c", ""]) assert.equal(parseSemver(bad), null);
});

test("compareSemver orders numerically, not lexically", () => {
  assert.ok(compareSemver(parseSemver("0.10.0"), parseSemver("0.9.9")) > 0);
  assert.ok(compareSemver(parseSemver("1.0.0"), parseSemver("0.99.99")) > 0);
  assert.equal(compareSemver(parseSemver("1.2.3"), parseSemver("1.2.3")), 0);
});

test("bumpVersion resets lower parts, also below 1.0", () => {
  assert.equal(bumpVersion("0.4.7", "patch"), "0.4.8");
  assert.equal(bumpVersion("0.4.7", "minor"), "0.5.0");
  assert.equal(bumpVersion("0.4.7", "major"), "1.0.0");
  assert.equal(bumpVersion("1.2.3", "major"), "2.0.0");
  assert.throws(() => bumpVersion("1.2.3", "huge"));
  assert.throws(() => bumpVersion("nope", "patch"));
});

test("currentVersionFromTags picks the highest v-tag and ignores the rest", () => {
  assert.equal(currentVersionFromTags([]), "0.0.0");
  assert.equal(currentVersionFromTags(["latest", "v1.2", "v1.0.0-rc.1", "x1.0.0"]), "0.0.0");
  assert.equal(currentVersionFromTags(["v0.2.0", "v0.10.0", "v0.9.5", "foo"]), "0.10.0");
  assert.equal(currentVersionFromTags(["v1.0.0\n", "v0.99.0"]), "1.0.0");
});

test("parseChangeset reads bump and body", () => {
  const c = parseChangeset("---\nbump: minor   # major | minor | patch\n---\nAdd Epic.\n\nMore.\n");
  assert.deepEqual(c, { bump: "minor", body: "Add Epic.\n\nMore." });
  assert.equal(parseChangeset("---\r\nbump: patch\r\n---\r\nFix.\r\n").body, "Fix.");
});

test("parseChangeset rejects malformed files", () => {
  assert.throws(() => parseChangeset("bump: minor\nText"), /header/);
  assert.throws(() => parseChangeset("---\nbump: minor\nText"), /not closed/);
  assert.throws(() => parseChangeset("---\nbump: huge\n---\nText"), /bump/);
  assert.throws(() => parseChangeset("---\n---\nText"), /bump/);
  assert.throws(() => parseChangeset("---\nbump: patch\n---\n  \n"), /empty/);
});

test("highestBump and nextVersion", () => {
  const cs = (...bumps) => bumps.map((bump) => ({ bump, body: "x" }));
  assert.equal(highestBump([]), null);
  assert.equal(highestBump(cs("patch", "minor", "patch")), "minor");
  assert.equal(highestBump(cs("patch", "major", "minor")), "major");
  assert.equal(nextVersion("0.0.0", []), null);
  assert.equal(nextVersion("0.3.1", cs("patch", "minor")), "0.4.0");
  assert.equal(nextVersion("0.3.1", cs("major")), "1.0.0");
  assert.equal(nextVersion("0.0.0", cs("patch")), "0.0.1");
});

test("renderNotes groups by bump with multi-line bullets", () => {
  const notes = renderNotes([
    { bump: "patch", body: "Fix a crash." },
    { bump: "major", body: "New library format.\nRe-scan after updating." },
    { bump: "minor", body: "Add GOG." },
    { bump: "minor", body: "Add Xbox." },
  ]);
  assert.equal(
    notes,
    [
      "### Major changes",
      "",
      "- New library format.",
      "  Re-scan after updating.",
      "",
      "### Minor changes",
      "",
      "- Add GOG.",
      "- Add Xbox.",
      "",
      "### Patch changes",
      "",
      "- Fix a crash.",
      "",
    ].join("\n"),
  );
});

test("setTomlVersion edits only the named section", () => {
  const toml = '[workspace.package]\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\nversion = "9"\n';
  const out = setTomlVersion(toml, "workspace.package", "0.2.0");
  assert.equal(out, toml.replace('"0.1.0"', '"0.2.0"'));
  const inherited = '[package]\nname = "a"\nversion.workspace = true\n';
  assert.equal(setTomlVersion(inherited, "package", "1.0.0"), inherited);
  assert.equal(setTomlVersion(inherited, "missing", "1.0.0"), inherited);
  assert.equal(
    setTomlVersion('[package]\r\nname = "a"\r\nversion = "0.1.0"\r\n', "package", "0.3.0"),
    '[package]\r\nname = "a"\r\nversion = "0.3.0"\r\n',
  );
});

test("setJsonVersion keeps formatting and skips nested versions", () => {
  const json = '{\n  "name": "x",\n  "version": "0.1.0",\n  "nested": {\n    "version": "5"\n  }\n}\n';
  assert.equal(setJsonVersion(json, "1.2.3"), json.replace('"0.1.0"', '"1.2.3"'));
  assert.throws(() => setJsonVersion("{}", "1.0.0"));
});

test("setCargoLockVersions touches only the listed packages", () => {
  const lock = [
    '[[package]]\nname = "questkeep"\nversion = "0.1.0"\ndependencies = [\n "serde",\n]\n',
    '[[package]]\nname = "questkeep-core"\nversion = "0.1.0"\n',
    '[[package]]\nname = "serde"\nversion = "0.1.0"\n',
  ].join("\n");
  const out = setCargoLockVersions(lock, ["questkeep", "questkeep-core"], "0.2.0");
  assert.match(out, /name = "questkeep"\nversion = "0.2.0"/);
  assert.match(out, /name = "questkeep-core"\nversion = "0.2.0"/);
  assert.match(out, /name = "serde"\nversion = "0.1.0"/);
});

test("updaterPlatform and buildLatestJson", () => {
  assert.equal(updaterPlatform("Questkeep_0.2.0_x64-setup.exe"), "windows-x86_64");
  assert.equal(updaterPlatform("Questkeep_0.2.0_amd64.AppImage"), "linux-x86_64");
  assert.equal(updaterPlatform("Questkeep_0.2.0_x64_en-US.msi"), null);
  const json = buildLatestJson({
    version: "0.2.0",
    notes: "n",
    pubDate: "2026-01-01T00:00:00.000Z",
    repo: "o/r",
    assets: [{ platform: "windows-x86_64", file: "Q_x64-setup.exe", signature: "SIG\n" }],
  });
  assert.deepEqual(json, {
    version: "0.2.0",
    notes: "n",
    pub_date: "2026-01-01T00:00:00.000Z",
    platforms: {
      "windows-x86_64": {
        signature: "SIG",
        url: "https://github.com/o/r/releases/download/v0.2.0/Q_x64-setup.exe",
      },
    },
  });
});
