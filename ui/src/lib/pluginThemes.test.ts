import { readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginInfo } from "../bindings/PluginInfo";
import type { UserTheme } from "../bindings/UserTheme";
import { makePlugin } from "../test-utils";
import baseCss from "../theme.css?raw";
import * as app from "./api";
import { contrastRatio } from "./color";
import { resolveColor, rootVars } from "./cssVars";
import {
  MAX_THEME_CSS_BYTES,
  parseThemeEntry,
  parseThemeParams,
  pluginThemeDef,
  pluginThemeProblems,
  resetPluginThemes,
  rewriteCssUrls,
  setThemeFetcher,
  syncPluginThemes,
  type Fetcher,
} from "./pluginThemes";
import { contrastFailures } from "./themeContrast";
import {
  applyAppearance,
  DEFAULT_APPEARANCE,
  getTheme,
  paramProperties,
  themes,
  type Scheme,
  type ThemeDef,
} from "./themes";
import { setSavedUserThemes } from "./userThemeStore";
import { emptyOverrides } from "./userThemes";

vi.mock("./api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

const css = (id: string, bg = "#010203") =>
  `:root[data-theme="${id}"] { --qk-color-bg: ${bg}; --qk-color-surface: #111111; --qk-color-text: #eeeeee; }`;

const entry = (over: Record<string, unknown> = {}) => ({
  id: "neon",
  name: "Neon",
  css: "theme.css",
  ...over,
});

const plugin = (overrides: Partial<PluginInfo> = {}): PluginInfo =>
  makePlugin({
    id: "neon",
    name: "Neon Pack",
    author: "Ada",
    providers: [],
    themes: 1,
    themeDefs: [entry()],
    revision: 1,
    ...overrides,
  });

/** A fetcher that answers from a map of URL endings; unknown files fail like a 404. */
function filesOf(files: Record<string, string>): ReturnType<typeof vi.fn<Fetcher>> {
  return vi.fn<Fetcher>((url) => {
    const found = Object.entries(files).find(([end]) => url.split("?")[0]?.endsWith(end));
    return found ? Promise.resolve(found[1]) : Promise.reject(new Error("HTTP 404"));
  });
}

const pageCss = () => document.getElementById("qk-theme")?.textContent ?? "";
const pluginThemes = () =>
  themes()
    .filter((t) => t.origin === "plugin")
    .map((t) => t.id);

let warnings: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
  vi.clearAllMocks();
  resetPluginThemes();
  vi.mocked(app.pluginFileUrl).mockImplementation((id, path) => `qk://plugins/${id}/${path}`);
  warnings = vi.spyOn(console, "warn").mockImplementation(() => undefined);
  applyAppearance({ ...DEFAULT_APPEARANCE, scheme: "dark" });
});
afterEach(() => {
  warnings.mockRestore();
  setThemeFetcher(null);
  resetPluginThemes();
  setSavedUserThemes([]);
  applyAppearance({ ...DEFAULT_APPEARANCE, scheme: "dark" });
});

/** Why an entry (the third of its manifest) is not accepted; null when it is. */
const error = (pluginId: string, raw: unknown) => {
  const parsed = parseThemeEntry(pluginId, raw, 2);
  return parsed.ok ? null : parsed.problem;
};

describe("parseThemeEntry", () => {
  it("accepts what the theme editor exports", () => {
    const parsed = parseThemeEntry("neon-theme", {
      id: "neon-theme",
      name: " Neon ",
      css: "./theme.css",
      params: [{ key: "accent", label: "Accent color", type: "color", default: "#F0A" }],
      preview: {
        light: ["#fff", "#eee", "#000", "#f0a"],
        dark: ["#000", "#111", "#fff", "#ff00aa80"],
      },
    });
    expect(parsed).toEqual({
      ok: true,
      droppedParams: 0,
      entry: {
        id: "neon-theme",
        name: "Neon",
        css: "theme.css",
        params: [{ key: "accent", label: "Accent color", type: "color", default: "#ff00aa" }],
        preview: {
          light: ["#ffffff", "#eeeeee", "#000000", "#ff00aa"],
          dark: ["#000000", "#111111", "#ffffff", "#ff00aa80"],
        },
      },
    });
  });

  it("wants the id to be the plugin's or to start with it", () => {
    expect(parseThemeEntry("neon", entry({ id: "neon-dark" })).ok).toBe(true);
    expect(error("neon", entry({ id: "neonx" }))?.error).toContain("“neon-”");
    expect(error("neon", entry({ id: "fjord" }))?.error).toContain("“fjord”");
    expect(error("neon", entry({ id: "Neon" }))?.error).toMatch(/lowercase/);
    expect(error("n", entry({ id: "n" }))?.error).toMatch(/lowercase/);
    expect(error("neon", entry({ id: `neon-${"a".repeat(60)}` }))?.error).toMatch(/lowercase/);
    // Ids of the user's own themes are reserved.
    expect(error("user-x", entry({ id: "user-x" }))?.error).toContain("“user-x”");
  });

  it("names a broken entry by its name, its id or its position", () => {
    expect(error("neon", "theme.css")).toEqual({
      name: "#3",
      error: "its entry in plugin.json is not an object",
    });
    expect(error("neon", { id: "neon", css: "theme.css" })).toEqual({
      name: "neon",
      error: "it has no name",
    });
    expect(error("neon", entry({ css: undefined }))?.name).toBe("Neon");
  });

  it("only takes a CSS file inside the plugin folder", () => {
    const paths = ["", "../theme.css", "/theme.css", "C:/theme.css", "a\\b.css", "a//b.css"];
    expect(paths.map((path) => error("neon", entry({ css: path }))?.error)).toEqual(
      paths.map(() => "“css” must name a file inside the plugin folder"),
    );
    expect(error("neon", entry({ css: 5 }))?.error).toMatch(/inside the plugin folder/);
    const nested = parseThemeEntry("neon", entry({ css: "styles/dark/theme.css" }));
    expect(nested.ok && nested.entry.css).toBe("styles/dark/theme.css");
  });

  it("leaves out a broken preview and a blank description", () => {
    const parsed = parseThemeEntry(
      "neon",
      entry({ description: "  ", preview: { light: ["#fff", "#eee", "#000"], dark: "x" } }),
    );
    expect(parsed.ok && parsed.entry).toEqual({
      id: "neon",
      name: "Neon",
      css: "theme.css",
      params: [],
    });
  });
});

describe("parseThemeParams", () => {
  it("normalizes every kind of parameter", () => {
    const { params, dropped } = parseThemeParams([
      {
        key: "accent",
        label: "Accent",
        type: "color",
        default: { light: "#ABC", dark: "#11223344" },
      },
      {
        key: "size",
        label: "Size",
        type: "number",
        default: 99,
        min: 5,
        max: 20,
        step: 0.5,
        unit: "px",
      },
      {
        key: "side",
        label: "Side",
        type: "select",
        default: "left",
        advanced: true,
        hint: "Where.",
        options: [{ value: "left", label: "Left" }, { value: "right" }],
      },
      { key: "glow", label: "Glow", type: "toggle", default: true, extra: "ignored" },
    ]);
    expect(dropped).toBe(0);
    expect(params).toEqual([
      {
        key: "accent",
        label: "Accent",
        type: "color",
        default: { light: "#aabbcc", dark: "#112233" },
      },
      {
        key: "size",
        label: "Size",
        type: "number",
        default: 20,
        min: 5,
        max: 20,
        step: 0.5,
        unit: "px",
      },
      {
        key: "side",
        label: "Side",
        type: "select",
        default: "left",
        advanced: true,
        hint: "Where.",
        options: [
          { value: "left", label: "Left" },
          { value: "right", label: "right" },
        ],
      },
      { key: "glow", label: "Glow", type: "toggle", default: true },
    ]);
  });

  it("drops malformed parameters and repeated keys", () => {
    const good = { key: "glow", label: "Glow", type: "toggle", default: false };
    const bad: unknown[] = [
      null,
      "glow",
      { ...good, key: "has space" },
      { ...good, key: "a;b" },
      { ...good, key: "" },
      { ...good, label: "" },
      { ...good, label: 5 },
      { ...good, type: "slider" },
      { ...good, default: "yes" },
      { key: "c", label: "C", type: "color", default: "red" },
      { key: "c", label: "C", type: "color", default: { light: "#fff" } },
      { key: "n", label: "N", type: "number", default: 1, min: 5, max: 2 },
      { key: "n", label: "N", type: "number", default: "1", min: 0, max: 2 },
      { key: "n", label: "N", type: "number", default: 1, min: 0, max: Infinity },
      { key: "s", label: "S", type: "select", default: "x", options: [{ value: "y", label: "Y" }] },
      { key: "s", label: "S", type: "select", default: "x", options: [] },
      { key: "s", label: "S", type: "select", default: 'a"]', options: [{ value: 'a"]' }] },
      {
        key: "s",
        label: "S",
        type: "select",
        default: "a",
        options: [{ value: "a" }, { value: "a" }],
      },
      { ...good, key: "GLOW" },
    ];
    expect(parseThemeParams([good, ...bad])).toEqual({ params: [good], dropped: bad.length });
    expect(parseThemeParams(undefined)).toEqual({ params: [], dropped: 0 });
    expect(parseThemeParams({ key: "x" })).toEqual({ params: [], dropped: 1 });
    // A unit or step that is no use is left out, the parameter stays.
    expect(
      parseThemeParams([
        {
          key: "n",
          label: "N",
          type: "number",
          default: 1,
          min: 0,
          max: 2,
          unit: "px;}",
          step: -1,
        },
      ]).params,
    ).toEqual([{ key: "n", label: "N", type: "number", default: 1, min: 0, max: 2 }]);
  });
});

const rewrite = (source: string, path = "theme.css") =>
  rewriteCssUrls(source, path, (file) => `qk://neon/@3/${file}`);

describe("rewriteCssUrls", () => {
  it("points relative references at the plugin's files", () => {
    expect(rewrite("a { background: url(img/bg.png) }")).toBe(
      'a { background: url("qk://neon/@3/img/bg.png") }',
    );
    expect(rewrite(`@font-face { src: url( './fonts/a b.woff2' ) format("woff2"); }`)).toBe(
      `@font-face { src: url("qk://neon/@3/fonts/a b.woff2") format("woff2"); }`,
    );
    expect(rewrite('a { mask: url("icons.svg#star"); b: URL(x.png?v=2) }')).toBe(
      'a { mask: url("qk://neon/@3/icons.svg#star"); b: url("qk://neon/@3/x.png?v=2") }',
    );
  });

  it("resolves them against the CSS file's folder", () => {
    expect(rewrite("a { b: url(../img/bg.png) }", "styles/dark/theme.css")).toBe(
      'a { b: url("qk://neon/@3/styles/img/bg.png") }',
    );
    expect(rewrite("a { b: url(bg%20one.png) }", "styles/theme.css")).toBe(
      'a { b: url("qk://neon/@3/styles/bg one.png") }',
    );
    // Out of the plugin folder: left alone (it will not load).
    expect(rewrite("a { b: url(../../secret.png) }", "styles/theme.css")).toBe(
      "a { b: url(../../secret.png) }",
    );
  });

  it("leaves absolute references, fragments and @import alone", () => {
    const untouched = [
      'a { b: url("data:image/png;base64,AAAA") }',
      "a { b: url(https://example.com/a.png) }",
      "a { b: url(HTTP://example.com/a.png) }",
      "a { b: url(qkplugin://localhost/other/a.png) }",
      "a { b: url(//example.com/a.png) }",
      "a { b: url(/a.png) }",
      "a { filter: url(#blur) }",
      "a { b: url() }",
      '@import url("more.css");',
      "a { color: red; }\n@import url(more.css) screen;",
    ];
    expect(untouched.map((source) => rewrite(source))).toEqual(untouched);
    // A url() after an @import statement is rewritten as usual.
    expect(rewrite('@import "x.css"; a { b: url(y.png) }')).toBe(
      '@import "x.css"; a { b: url("qk://neon/@3/y.png") }',
    );
  });
});

describe("syncPluginThemes", () => {
  it("registers the themes of enabled plugins with their CSS, author and plugin", async () => {
    const fetcher = filesOf({ "theme.css": `${css("neon")} .tile { mask: url(img/m.svg); }` });
    setThemeFetcher(fetcher);
    await syncPluginThemes([
      plugin({
        themeDefs: [
          entry({
            description: "Bright.",
            params: [{ key: "glow", label: "Glow", type: "toggle", default: true }, { key: "?" }],
          }),
        ],
      }),
      plugin({ id: "off", enabled: false, themeDefs: [] }),
    ]);

    expect(fetcher).toHaveBeenCalledExactlyOnceWith("qk://plugins/neon/@1/theme.css?v=1");
    expect(getTheme("neon")).toEqual({
      id: "neon",
      name: "Neon",
      description: "Bright.",
      author: "Ada",
      css: `${css("neon")} .tile { mask: url("qk://plugins/neon/@1/img/m.svg"); }`,
      params: [{ key: "glow", label: "Glow", type: "toggle", default: true }],
      origin: "plugin",
      plugin: { id: "neon", name: "Neon Pack" },
      // No preview in the manifest: computed from the CSS; the accent is the base palette's.
      preview: {
        light: ["#010203", "#111111", "#eeeeee", "#3d5fd9"],
        dark: ["#010203", "#111111", "#eeeeee", "#7c9cff"],
      },
    });
    expect(pluginThemeProblems()).toEqual({});
    expect(warnings).toHaveBeenCalledTimes(1);
  });

  it("keeps the manifest's preview and reads several themes of one plugin", async () => {
    const preview = {
      light: ["#ffffff", "#eeeeee", "#000000", "#ff00aa"],
      dark: ["#000000", "#111111", "#ffffff", "#ff00aa"],
    };
    setThemeFetcher(filesOf({ "a.css": css("neon"), "b.css": css("neon-two") }));
    await syncPluginThemes([
      plugin({
        author: null,
        themeDefs: [
          entry({ css: "a.css", preview }),
          entry({ id: "neon-two", name: "Two", css: "b.css" }),
        ],
      }),
    ]);
    expect(pluginThemes()).toEqual(["neon", "neon-two"]);
    expect(getTheme("neon")?.preview).toEqual(preview);
    expect(getTheme("neon")).not.toHaveProperty("author");
  });

  it("reports themes that cannot be loaded and loads the others", async () => {
    setThemeFetcher(
      filesOf({ "ok.css": css("neon-ok"), "big.css": "a".repeat(MAX_THEME_CSS_BYTES + 1) }),
    );
    await syncPluginThemes([
      plugin({
        themeDefs: [
          entry({ id: "neon-ok", name: "Fine", css: "ok.css" }),
          entry({ id: "neon-gone", name: "Gone", css: "missing.css" }),
          entry({ id: "neon-big", name: "Big", css: "big.css" }),
          entry({ id: "other", name: "Foreign" }),
          entry({ id: "neon-ok", name: "Twice", css: "ok.css" }),
          null as unknown as Record<string, unknown>,
        ],
      }),
    ]);
    expect(pluginThemes()).toEqual(["neon-ok"]);
    expect(pluginThemeProblems()).toEqual({
      neon: [
        { name: "Gone", error: "missing.css: HTTP 404" },
        { name: "Big", error: "big.css: the CSS file is larger than 1 MiB" },
        { name: "Foreign", error: "its id “other” must be “neon” or start with “neon-”" },
        { name: "Twice", error: "another theme already has the id “neon-ok”" },
        { name: "#6", error: "its entry in plugin.json is not an object" },
      ],
    });
    // The problems go with the plugin.
    await syncPluginThemes([]);
    expect(pluginThemeProblems()).toEqual({});
    expect(pluginThemes()).toEqual([]);
  });

  it("never replaces a built-in theme or another plugin's theme", async () => {
    setThemeFetcher(filesOf({ "theme.css": css("x") }));
    await syncPluginThemes([
      plugin({ id: "fjord", themeDefs: [entry({ id: "fjord" })] }),
      plugin({ id: "neon", themeDefs: [entry({ id: "neon-extra" })] }),
      plugin({ id: "neon-extra", themeDefs: [entry({ id: "neon-extra" })] }),
    ]);
    expect(getTheme("fjord")?.origin).toBe("builtin");
    expect(pluginThemeProblems().fjord).toEqual([
      { name: "Neon", error: "another theme already has the id “fjord”" },
    ]);
    expect(getTheme("neon-extra")?.plugin?.id).toBe("neon");
    expect(pluginThemeProblems()["neon-extra"]).toHaveLength(1);
  });

  it("reads the CSS again when the revision changes, not otherwise", async () => {
    const fetcher = filesOf({ "theme.css": css("neon") });
    setThemeFetcher(fetcher);
    await syncPluginThemes([plugin()]);
    await syncPluginThemes([plugin()]);
    expect(fetcher).toHaveBeenCalledTimes(1);

    fetcher.mockResolvedValue(css("neon", "#0a0b0c"));
    await syncPluginThemes([plugin({ revision: 2, themeDefs: [entry({ name: "Neon 2" })] })]);
    expect(fetcher).toHaveBeenLastCalledWith("qk://plugins/neon/@2/theme.css?v=2");
    expect(getTheme("neon")).toMatchObject({ name: "Neon 2", css: css("neon", "#0a0b0c") });
    expect(pluginThemes()).toEqual(["neon"]);
  });

  it("re-applies the chosen theme on the page when its plugin is loaded again", async () => {
    const fetcher = filesOf({ "theme.css": css("neon") });
    setThemeFetcher(fetcher);
    applyAppearance({ ...DEFAULT_APPEARANCE, scheme: "dark", theme: "neon" });
    // Not there yet: the default theme shows.
    expect(document.documentElement.dataset.theme).toBe("questkeep");
    await syncPluginThemes([plugin()]);
    expect(document.documentElement.dataset.theme).toBe("neon");
    expect(pageCss()).toBe(css("neon"));

    fetcher.mockResolvedValue(css("neon", "#0a0b0c"));
    const seen: string[] = [];
    const observer = new MutationObserver(() =>
      seen.push(document.documentElement.dataset.theme ?? ""),
    );
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    await syncPluginThemes([plugin({ revision: 2 })]);
    observer.disconnect();
    expect(pageCss()).toBe(css("neon", "#0a0b0c"));
    expect(document.documentElement.dataset.theme).toBe("neon");
    // Replaced in place: the page never went through the default theme.
    expect(seen).not.toContain("questkeep");

    // Disabled: the default theme shows, the choice stays for when it comes back.
    await syncPluginThemes([plugin({ revision: 2, enabled: false, loaded: false, themeDefs: [] })]);
    expect(getTheme("neon")).toBeUndefined();
    expect(document.documentElement.dataset.theme).toBe("questkeep");
    expect(pageCss()).not.toContain("#0a0b0c");
  });

  it("drops themes a reload no longer has", async () => {
    setThemeFetcher(filesOf({ "theme.css": css("neon") }));
    await syncPluginThemes([
      plugin({ themeDefs: [entry(), entry({ id: "neon-two", name: "Two" })] }),
    ]);
    expect(pluginThemes()).toEqual(["neon", "neon-two"]);
    await syncPluginThemes([
      plugin({ revision: 2, themeDefs: [entry({ id: "neon-two", name: "Two" })] }),
    ]);
    expect(pluginThemes()).toEqual(["neon-two"]);
  });

  it("does not register what a reload overtook while it was being read", async () => {
    const answers: ((text: string) => void)[] = [];
    const fetcher = vi.fn<Fetcher>(() => new Promise<string>((resolve) => answers.push(resolve)));
    setThemeFetcher(fetcher);
    const first = syncPluginThemes([plugin({ revision: 1 })]);
    await vi.waitFor(() => expect(answers).toHaveLength(1));
    const second = syncPluginThemes([plugin({ revision: 2 })]);
    const third = syncPluginThemes([plugin({ revision: 2 })]);
    // The first revision's file arrives only now.
    answers[0]?.(css("neon", "#111111"));
    await first;
    expect(getTheme("neon")).toBeUndefined();
    await vi.waitFor(() => expect(answers).toHaveLength(2));
    answers[1]?.(css("neon", "#222222"));
    await Promise.all([second, third]);
    expect(getTheme("neon")?.css).toBe(css("neon", "#222222"));
    expect(fetcher.mock.calls.map(([url]) => url)).toEqual([
      "qk://plugins/neon/@1/theme.css?v=1",
      "qk://plugins/neon/@2/theme.css?v=2",
    ]);
  });

  it("removes the themes of a plugin that went away while they were being read", async () => {
    const answers: ((text: string) => void)[] = [];
    setThemeFetcher(() => new Promise<string>((resolve) => answers.push(resolve)));
    const first = syncPluginThemes([plugin()]);
    await vi.waitFor(() => expect(answers).toHaveLength(1));
    const second = syncPluginThemes([]);
    answers[0]?.(css("neon"));
    await Promise.all([first, second]);
    expect(pluginThemes()).toEqual([]);
    expect(pluginThemeProblems()).toEqual({});
  });

  it("gives user themes that build on a plugin theme its CSS", async () => {
    const mine: UserTheme = {
      id: "user-mine",
      name: "Mine",
      base: "neon",
      overrides: emptyOverrides(),
      css: "",
    };
    setSavedUserThemes([mine]);
    // Its base is missing: it builds on the default theme meanwhile.
    expect(getTheme("user-mine")?.css).not.toContain("#010203");

    const fetcher = filesOf({ "theme.css": css("neon") });
    setThemeFetcher(fetcher);
    await syncPluginThemes([plugin()]);
    expect(getTheme("user-mine")?.css).toContain(css("user-mine"));

    fetcher.mockResolvedValue(css("neon", "#0a0b0c"));
    await syncPluginThemes([plugin({ revision: 2 })]);
    expect(getTheme("user-mine")?.css).toContain("#0a0b0c");

    await syncPluginThemes([]);
    expect(getTheme("user-mine")?.css).not.toContain("#0a0b0c");
  });

  it("copes with plugins that have no theme list", async () => {
    const fetcher = filesOf({});
    setThemeFetcher(fetcher);
    await syncPluginThemes([
      { ...plugin(), themeDefs: undefined } as unknown as PluginInfo,
      plugin({ id: "other", themeDefs: [] }),
    ]);
    await syncPluginThemes(null);
    expect(fetcher).not.toHaveBeenCalled();
    expect(pluginThemes()).toEqual([]);
  });
});

/** A file next to this one (the path is a parameter, so Vite leaves the URL alone). */
const fileUrl = (path: string) => new URL(path, import.meta.url);

/** The variables `<html>` has for a theme in a scheme, with the given saved parameters. */
function varsOf(theme: ThemeDef, scheme: Scheme, saved: Record<string, string | boolean> = {}) {
  const { vars, attrs } = paramProperties(theme, saved, scheme);
  return rootVars(
    [baseCss, theme.css],
    { "data-scheme": scheme, "data-theme": theme.id, ...attrs },
    vars,
  );
}

describe("the example theme plugin", () => {
  const FOLDER = "../../../examples/plugins/example-theme/";
  const read = (file: string) =>
    readFileSync(fileUrl(FOLDER + file), "utf8").replace(/\r\n/g, "\n");
  const manifest = JSON.parse(read("plugin.json")) as {
    id: string;
    name: string;
    author: string;
    themes: unknown[];
  };

  /** The example's theme, through the same checks and steps as an installed plugin's. */
  function load(): ThemeDef {
    const parsed = parseThemeEntry(manifest.id, manifest.themes[0]);
    if (!parsed.ok) throw new Error(parsed.problem.error);
    expect(parsed.droppedParams).toBe(0);
    return pluginThemeDef(manifest, parsed.entry, read(parsed.entry.css), (file) => {
      // Every file the CSS refers to is in the plugin folder.
      expect(read(file)).not.toBe("");
      return `qk://example-theme/@1/${file}`;
    });
  }

  it("is a valid theme plugin", () => {
    expect(manifest.themes).toHaveLength(1);
    const theme = load();
    expect(theme).toMatchObject({
      id: "example-theme",
      name: "Arcade",
      author: "Questkeep",
      origin: "plugin",
      plugin: { id: "example-theme", name: "Example Theme" },
    });
    expect(theme.description).toBeTruthy();
    expect(theme.params.map((p) => [p.key, p.type, p.advanced === true])).toEqual([
      ["accent", "color", false],
      ["glow", "toggle", false],
      ["sidebar", "select", true],
    ]);
    expect(theme.css).toContain('url("qk://example-theme/@1/assets/scanlines.svg")');
    expect(theme.css).not.toContain("@import");
  });

  it("scopes every rule to its theme", () => {
    const selectors = load()
      .css.replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/\{[^{}]*\}/g, "\n")
      .split(/[\n,]/)
      .map((s) => s.trim())
      .filter((s) => s !== "");
    expect(selectors.length).toBeGreaterThan(8);
    expect(selectors.filter((s) => !s.startsWith(':root[data-theme="example-theme"]'))).toEqual([]);
  });

  it.each<Scheme>(["dark", "light"])("%s: text and accent are readable", (scheme) => {
    const theme = load();
    expect(contrastFailures(varsOf(theme, scheme))).toEqual([]);
    expect(contrastFailures(varsOf(theme, scheme, { glow: false, sidebar: "right" }))).toEqual([]);
    // Any accent the user picks still gets readable text on it.
    const weak = ["#ffffff", "#000000", "#ff0000", "#00ff00", "#808080"].filter((accent) => {
      const vars = varsOf(theme, scheme, { accent });
      const ratio = contrastRatio(
        resolveColor("--qk-color-accent-contrast", vars),
        resolveColor("--qk-color-accent", vars),
      );
      return ratio < 4.5;
    });
    expect(weak).toEqual([]);
  });

  it("has different light and dark palettes and a preview that shows them", () => {
    const theme = load();
    const parsed = parseThemeEntry(manifest.id, manifest.themes[0]);
    // What the manifest says is what the CSS gives.
    const { preview: _written, ...withoutPreview } = (parsed.ok ? parsed.entry : {}) as {
      preview?: unknown;
    };
    const computed = pluginThemeDef(
      manifest,
      withoutPreview as Parameters<typeof pluginThemeDef>[1],
      read("theme.css"),
      (file) => file,
    ).preview;
    expect(theme.preview).toEqual(computed);
    expect(theme.preview?.light).not.toEqual(theme.preview?.dark);
  });

  it("moves the sidebar with grid placement and reacts to its options", () => {
    const theme = load();
    expect(theme.css).toMatch(
      /\[data-param-sidebar="right"\] \.app \{\s*grid-template-columns: minmax\(0, 1fr\) var\(--qk-sidebar-width\);/,
    );
    expect(theme.css).toMatch(/\[data-param-sidebar="right"\] \.sidebar \{\s*grid-column: 2;/);
    expect(varsOf(theme, "dark").get("--qk-titlebar-height")).toBe("56px");
    const glow = varsOf(theme, "dark").get("--qk-shadow-tile-hover");
    const plain = varsOf(theme, "dark", { glow: false }).get("--qk-shadow-tile-hover");
    expect(glow).toContain("var(--qk-color-accent)");
    expect(plain).not.toBe(glow);
  });
});
