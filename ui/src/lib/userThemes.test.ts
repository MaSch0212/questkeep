import { describe, expect, it } from "vitest";
import type { UserTheme } from "../bindings/UserTheme";
import { getTheme, type ThemeDef } from "./themes";
import {
  emptyOverrides,
  exportedTheme,
  filterVariables,
  hasBalancedBraces,
  humanizeVariable,
  isOverrideValue,
  isPerScheme,
  isUserThemeId,
  isVariableName,
  overrideCount,
  overridesAccent,
  overridesCss,
  overrideSource,
  overrideValue,
  pluginThemeId,
  resolvedHex,
  retargetCss,
  scopedCss,
  stepValue,
  themeVariables,
  uniqueName,
  userThemeDef,
  userThemeId,
  userThemeVars,
  VARIABLE_GROUPS,
  variableGroup,
  variableKind,
  withOverride,
  withoutOverride,
} from "./userThemes";

const fjord = getTheme("fjord") as ThemeDef;
const questkeep = getTheme("questkeep") as ThemeDef;

const theme = (over: Partial<UserTheme> = {}): UserTheme => ({
  id: "user-neon",
  name: "Neon",
  base: "fjord",
  overrides: emptyOverrides(),
  css: "",
  ...over,
});

describe("user theme ids and names", () => {
  it("makes ids from names and avoids taken ones", () => {
    expect(userThemeId("My Théme #1!", [])).toBe("user-my-theme-1");
    expect(userThemeId("  ", [])).toBe("user-theme");
    expect(userThemeId("Neon", ["user-neon", "user-neon-2"])).toBe("user-neon-3");
    const long = userThemeId("x".repeat(100), []);
    expect(isUserThemeId(long)).toBe(true);
    expect(isUserThemeId("user-")).toBe(false);
    expect(isUserThemeId("fjord")).toBe(false);
    expect(isUserThemeId("user-Neon")).toBe(false);
    expect(pluginThemeId("user-my-theme")).toBe("my-theme-theme");
    expect(uniqueName("Neon", ["Neon", "Neon 2"])).toBe("Neon 3");
    expect(uniqueName("Neon", [])).toBe("Neon");
  });

  it("accepts only values that stay inside a declaration", () => {
    expect(isOverrideValue("#fff")).toBe(true);
    expect(isOverrideValue("color-mix(in oklab, #000 80%, red)")).toBe(true);
    expect(isOverrideValue('"Segoe UI", sans-serif')).toBe(true);
    for (const bad of ["", "  ", "red; color: blue", "red }", "{", "x</style>", "a\nb"]) {
      expect(isOverrideValue(bad)).toBe(false);
    }
    expect(isOverrideValue("x".repeat(513))).toBe(false);
    expect(isVariableName("--qk-color-bg")).toBe(true);
    expect(isVariableName("--color-bg")).toBe(false);
    expect(isVariableName("--qk-a b")).toBe(false);
  });
});

describe("overrides", () => {
  it("sets a variable for one mode or both, and reads it back", () => {
    let overrides = withOverride(emptyOverrides(), "--qk-color-bg", "#111", "dark", "dark");
    expect(overrides.dark).toEqual({ "--qk-color-bg": "#111" });
    expect(overrideValue(overrides, "--qk-color-bg", "dark")).toBe("#111");
    expect(overrideValue(overrides, "--qk-color-bg", "light")).toBeUndefined();
    expect(overrideSource(overrides, "--qk-color-bg", "dark")).toBe("dark");
    expect(overrideSource(overrides, "--qk-color-bg", "light")).toBeNull();

    // For both, edited in dark: dark's own value goes (it would hide it), light's stays.
    overrides = withOverride(overrides, "--qk-color-bg", "#eee", "light", "light");
    overrides = withOverride(overrides, "--qk-color-bg", "#222", "both", "dark");
    expect(overrides).toEqual({
      both: { "--qk-color-bg": "#222" },
      light: { "--qk-color-bg": "#eee" },
      dark: {},
    });
    expect(overrideSource(overrides, "--qk-color-bg", "dark")).toBe("both");
    expect(overrideSource(overrides, "--qk-color-bg", "light")).toBe("light");
    expect(overrideCount(overrides)).toBe(1);

    // Reset in dark removes what dark shows; light keeps its own value.
    overrides = withoutOverride(overrides, "--qk-color-bg", "dark");
    expect(overrides).toEqual({ both: {}, light: { "--qk-color-bg": "#eee" }, dark: {} });
    expect(overridesAccent(overrides, "light")).toBe(false);
    expect(
      overridesAccent(withOverride(overrides, "--qk-color-accent", "red", "both", "dark"), "light"),
    ).toBe(true);
  });
});

describe("generated CSS", () => {
  it("re-targets the base theme's rules to the user theme", () => {
    const css = retargetCss(fjord.css, "fjord", "user-neon");
    expect(css).toContain(':root[data-theme="user-neon"][data-scheme="dark"]');
    expect(css).not.toContain('[data-theme="fjord"]');
    const def = userThemeDef(theme(), fjord);
    expect(def.css.trim()).toBe(css.trim());
    expect(def.id).toBe("user-neon");
    expect(def.name).toBe("Neon");
    expect(def.origin).toBe("user");
    // The base's parameters (and their translations) come along.
    expect(def.params).toBe(fjord.params);
    expect(def.text).toBe(fjord.text);
    expect(def.preview).toEqual(fjord.preview);
  });

  it("writes overrides as scheme blocks that outrank the base theme", () => {
    const overrides = {
      both: { "--qk-radius-scale": "2", "--qk-color-text": " #fafafa " },
      light: { "--qk-color-bg": "#ffeedd" },
      dark: { "--qk-color-bg": "#010203" },
    };
    expect(overridesCss("user-neon", overrides)).toBe(
      [
        ':root[data-theme="user-neon"][data-theme][data-theme][data-theme] {',
        "  --qk-color-text: #fafafa;",
        "  --qk-radius-scale: 2;",
        "}",
        ':root[data-theme="user-neon"][data-theme][data-theme][data-theme][data-scheme="light"] {',
        "  --qk-color-bg: #ffeedd;",
        "}",
        ':root[data-theme="user-neon"][data-theme][data-theme][data-theme][data-scheme="dark"] {',
        "  --qk-color-bg: #010203;",
        "}",
        "",
      ].join("\n"),
    );
    expect(overridesCss("user-neon", emptyOverrides())).toBe("");
    // Whatever slipped past validation never reaches the CSS.
    expect(
      overridesCss("user-neon", {
        both: { "--qk-color-bg": "red; } body { display: none", color: "red" },
        light: {},
        dark: {},
      }),
    ).toBe("");

    // Fjord sets its background per scheme; an override for both modes still wins in each.
    const custom = theme({ overrides });
    expect(resolvedHex("--qk-color-bg", userThemeVars(custom, fjord, "dark"))).toBe("#010203");
    expect(resolvedHex("--qk-color-bg", userThemeVars(custom, fjord, "light"))).toBe("#ffeedd");
    expect(resolvedHex("--qk-color-text", userThemeVars(custom, fjord, "light"))).toBe("#fafafa");
    expect(resolvedHex("--qk-color-text", userThemeVars(custom, fjord, "dark"))).toBe("#fafafa");
    // Without the overrides: what Reset goes back to.
    expect(
      resolvedHex("--qk-color-bg", userThemeVars(custom, fjord, "dark", { overrides: false })),
    ).toBe("#131a23");
    const def = userThemeDef(custom, fjord);
    expect(def.preview).toEqual({
      dark: ["#010203", "#18212c", "#fafafa", "#6cc5e8"],
      light: ["#ffeedd", "#f6f8fb", "#fafafa", "#0f6f96"],
    });
  });

  it("nests the free CSS in the theme's selector, last", () => {
    const css =
      '.btn { text-transform: uppercase; }\n&[data-scheme="dark"] .tile { opacity: 0.9; }';
    expect(scopedCss("user-neon", css)).toBe(`:root[data-theme="user-neon"] {\n${css}\n}\n`);
    expect(scopedCss("user-neon", "  \n")).toBe("");
    const def = userThemeDef(
      theme({ css, overrides: { ...emptyOverrides(), both: { "--qk-space-scale": "1.2" } } }),
      fjord,
    );
    expect(def.css.endsWith(`:root[data-theme="user-neon"] {\n${css}\n}\n`)).toBe(true);
    expect(def.css.indexOf("--qk-space-scale: 1.2")).toBeLessThan(def.css.indexOf(".btn {"));
    // A declaration inside the free CSS is not mistaken for one of the theme's variables.
    const sneaky = theme({ css: ".x { color: red; --qk-color-bg: #ff0000; }" });
    expect(userThemeDef(sneaky, fjord).preview?.dark[0]).toBe("#131a23");
  });

  it("leaves out free CSS with a stray closing brace", () => {
    expect(hasBalancedBraces(".a { color: red; }")).toBe(true);
    expect(hasBalancedBraces(".a { color: red;")).toBe(true);
    expect(hasBalancedBraces("} body { display: none; }")).toBe(false);
    expect(hasBalancedBraces(".a { } }")).toBe(false);
    expect(hasBalancedBraces('.a::after { content: "}"; } /* } */')).toBe(true);
    expect(scopedCss("user-neon", "} body { display: none; }")).toBe("");
    expect(userThemeDef(theme({ css: "} * { display: none }" }), fjord).css).not.toContain(
      "display",
    );
  });

  it("lets an accent override beat the accent parameter", () => {
    const custom = theme({
      overrides: { ...emptyOverrides(), dark: { "--qk-color-accent": "#ff0000" } },
    });
    const saved = { accent: "#00ff00" };
    expect(resolvedHex("--qk-color-accent", userThemeVars(custom, fjord, "dark", { saved }))).toBe(
      "#ff0000",
    );
    // In light mode there is no override: the parameter still decides, shades included.
    const light = userThemeVars(custom, fjord, "light", { saved });
    expect(resolvedHex("--qk-color-accent", light)).toBe("#00ff00");
    expect(resolvedHex("--qk-color-heat-4", light)).toBe("#00ff00");
    expect(overridesAccent(custom.overrides, "dark")).toBe(true);
    expect(overridesAccent(custom.overrides, "light")).toBe(false);
  });

  it("works on the default theme and without a base", () => {
    const onDefault = userThemeDef(theme({ base: "questkeep" }), questkeep);
    expect(onDefault.css).toContain(':root[data-theme="user-neon"][data-param-corners="sharp"]');
    expect(onDefault.params.map((p) => p.key)).toContain("density");
    expect(onDefault.preview).toEqual(questkeep.preview);
    const alone = userThemeDef(
      theme({ overrides: { ...emptyOverrides(), both: { "--qk-color-bg": "rgb(1 2 3)" } } }),
      undefined,
    );
    expect(alone.params).toEqual([]);
    // A color this app cannot compute falls back to grey in the preview; the CSS keeps it.
    expect(alone.preview?.dark[0]).toBe("#808080");
    expect(alone.css).toContain("--qk-color-bg: rgb(1 2 3);");
  });

  it("exports against the plugin's theme id with literal texts", () => {
    const custom = theme({
      base: "questkeep",
      css: ".btn { letter-spacing: 1px; }",
      overrides: { ...emptyOverrides(), both: { "--qk-color-bg": "#010203" } },
    });
    const exported = exportedTheme(custom, questkeep);
    expect(exported.css).toContain(':root[data-theme="neon-theme"][data-param-density="compact"]');
    expect(exported.css).toContain(':root[data-theme="neon-theme"] {\n.btn {');
    expect(exported.css).not.toContain("user-neon");
    expect(exported.css).not.toContain('[data-theme="questkeep"]');
    const params = exported.params as {
      key: string;
      label: string;
      options?: { label: string }[];
    }[];
    expect(params[0]).toMatchObject({ key: "accent", label: "Accent color", type: "color" });
    expect(params.find((p) => p.key === "corners")?.options?.map((o) => o.label)).toEqual([
      "Sharp",
      "Default",
      "Round",
    ]);
    expect(params.find((p) => p.key === "font")).toMatchObject({
      hint: "Uses the first matching font installed on this computer.",
    });
    expect(JSON.stringify(params)).not.toContain("optionDefault");
    expect(exported.preview).toEqual({
      dark: ["#010203", "#171a20", "#e6e8ec", "#7c9cff"],
      light: ["#010203", "#fafbfc", "#1b1e26", "#3d5fd9"],
    });
    expect(exportedTheme(custom, undefined).params).toEqual([]);
  });
});

describe("the catalogue of variables", () => {
  const variables = themeVariables();
  const find = (name: string) => variables.find((v) => v.name === name);
  const group = (name: string) => find(name)?.group;

  it("lists every variable of theme.css once, in file order, with both schemes' values", () => {
    expect(variables.length).toBeGreaterThan(300);
    expect(new Set(variables.map((v) => v.name)).size).toBe(variables.length);
    expect(variables[0]?.name).toBe("--qk-color-bg");
    expect(find("--qk-color-bg")?.base).toEqual({ dark: "#111317", light: "#f3f4f7" });
    expect(find("--qk-sidebar-width")?.base).toEqual({ dark: "220px", light: "220px" });
    expect(variables.every((v) => v.name.startsWith("--qk-"))).toBe(true);
    expect(variables.some((v) => v.name.startsWith("--qk-param-"))).toBe(false);
  });

  it("groups variables by the sections of theme.css", () => {
    expect(group("--qk-color-bg")).toBe("surfaces");
    expect(group("--qk-color-surface-hover")).toBe("surfaces");
    expect(group("--qk-color-border")).toBe("surfaces");
    expect(group("--qk-color-text-muted")).toBe("text");
    expect(group("--qk-color-accent")).toBe("accent");
    expect(group("--qk-color-group-band")).toBe("accent");
    expect(group("--qk-filter-accent-border")).toBe("accent");
    expect(group("--qk-color-danger-soft")).toBe("status");
    expect(group("--qk-color-running")).toBe("status");
    expect(group("--qk-color-series-3")).toBe("charts");
    expect(group("--qk-color-heat-2")).toBe("charts");
    expect(group("--qk-color-tile-title-fade")).toBe("cards");
    expect(group("--qk-placeholder-saturation")).toBe("cards");
    expect(group("--qk-font-size-md")).toBe("typography");
    expect(group("--qk-line-height")).toBe("typography");
    expect(group("--qk-radius-md")).toBe("shape");
    expect(group("--qk-space-3")).toBe("shape");
    expect(group("--qk-font-scale")).toBe("shape");
    expect(group("--qk-focus-ring-width")).toBe("shape");
    expect(group("--qk-shadow-dialog")).toBe("shadows");
    expect(group("--qk-duration-fast")).toBe("motion");
    expect(group("--qk-easing")).toBe("motion");
    expect(group("--qk-sidebar-width")).toBe("sizes");
    expect(group("--qk-z-modal")).toBe("sizes");
    expect(variableGroup("--qk-something-new")).toBe("sizes");
    // Every group is used, and the advanced one is the biggest by far.
    for (const name of VARIABLE_GROUPS) {
      expect(variables.some((v) => v.group === name)).toBe(true);
    }
    const sizes = variables.filter((v) => v.group === "sizes").length;
    expect(sizes).toBeGreaterThan(variables.length / 2);
  });

  it("tells the kind of a variable", () => {
    const kind = (name: string) => find(name)?.kind;
    expect(kind("--qk-color-bg")).toBe("color");
    expect(kind("--qk-color-accent-soft")).toBe("color");
    expect(kind("--qk-filter-accent-soft")).toBe("color");
    expect(kind("--qk-shadow-tile")).toBe("shadow");
    expect(kind("--qk-font-family")).toBe("font");
    expect(kind("--qk-sidebar-width")).toBe("length");
    expect(kind("--qk-duration-fast")).toBe("length");
    expect(kind("--qk-space-2")).toBe("length");
    expect(kind("--qk-font-size-md")).toBe("length");
    expect(kind("--qk-font-weight-bold")).toBe("number");
    expect(kind("--qk-radius-scale")).toBe("number");
    expect(kind("--qk-easing")).toBe("other");
    expect(variableKind("--qk-x", "2 / 3")).toBe("other");
    expect(isPerScheme(find("--qk-color-bg") as never)).toBe(true);
    expect(isPerScheme(find("--qk-shadow-tile") as never)).toBe(true);
    expect(isPerScheme(find("--qk-sidebar-width") as never)).toBe(false);
  });

  it("makes names readable", () => {
    expect(humanizeVariable("--qk-color-surface-hover")).toBe("Surface hover");
    expect(humanizeVariable("--qk-color-bg")).toBe("Background");
    expect(humanizeVariable("--qk-font-size-xs")).toBe("Font size xs");
    expect(humanizeVariable("--qk-sidebar-width")).toBe("Sidebar width");
    expect(humanizeVariable("--qk-color-ttb-main")).toBe("Time to beat main");
    expect(find("--qk-color-text-muted")?.label).toBe("Text muted");
  });

  it("filters by name or label, every word", () => {
    const names = (search: string) => filterVariables(variables, search).map((v) => v.name);
    expect(names("")).toHaveLength(variables.length);
    expect(names("surface hover")).toEqual(["--qk-color-surface-hover"]);
    expect(names("BACKGROUND")).toContain("--qk-color-bg");
    expect(names("--qk-radius")).toEqual([
      "--qk-radius-scale",
      "--qk-radius-sm",
      "--qk-radius-md",
      "--qk-radius-lg",
      "--qk-radius-pill",
    ]);
    expect(names("no such thing")).toEqual([]);
  });

  it("steps lengths and numbers", () => {
    expect(stepValue("220px", 1)).toBe("221px");
    expect(stepValue("220px", -1, true)).toBe("210px");
    expect(stepValue("1.4", 1)).toBe("1.5");
    expect(stepValue("0.06em", -1)).toBe("-0.04em");
    expect(stepValue("120ms", 1, true)).toBe("130ms");
    expect(stepValue("1", -1)).toBe("0");
    expect(stepValue("1s", 1)).toBe("1.1s");
    expect(stepValue(" 38% ", 1)).toBe("39%");
    expect(stepValue("calc(4px * 2)", 1)).toBeNull();
    expect(stepValue("auto", 1)).toBeNull();
  });
});
