import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Appearance } from "../bindings/Appearance";
import {
  activeThemeId,
  applyAppearance,
  applyCachedAppearance,
  applyResolved,
  cachedTheme,
  DEFAULT_APPEARANCE,
  effectiveScheme,
  getTheme,
  paramCssValue,
  paramDefault,
  paramProperties,
  registerTheme,
  resolveAppearance,
  resolveParams,
  resolveScheme,
  sanitizeParam,
  themes,
  themeText,
  unregisterTheme,
  withoutParams,
  withParam,
  type ThemeDef,
  type ThemeParam,
} from "./themes";

const accent: ThemeParam = {
  key: "accent",
  label: "Accent",
  type: "color",
  default: { light: "#112233", dark: "#AABBCC" },
};
const size: ThemeParam = {
  key: "size",
  label: "Size",
  type: "number",
  default: 10,
  min: 5,
  max: 20,
  unit: "px",
};
const shape: ThemeParam = {
  key: "shape",
  label: "Shape",
  type: "select",
  default: "round",
  options: [
    { value: "round", label: "Round" },
    { value: "square", label: "Square" },
  ],
};
const glow: ThemeParam = { key: "glow", label: "Glow", type: "toggle", default: false };

const neon = (over: Partial<ThemeDef> = {}): ThemeDef => ({
  id: "plugin.neon",
  name: "Neon",
  css: ':root[data-theme="plugin.neon"] { --qk-color-bg: #010203; }',
  params: [accent, size, shape, glow],
  origin: "plugin",
  ...over,
});

const appearance = (over: Partial<Appearance> = {}): Appearance => ({
  ...DEFAULT_APPEARANCE,
  ...over,
});

const root = document.documentElement;
const styleText = () => document.getElementById("qk-theme")?.textContent;

/** A `matchMedia` whose "prefers dark" answer the test can flip. */
function fakeMedia(dark: boolean) {
  const listeners = new Set<() => void>();
  const query = {
    matches: dark,
    addEventListener: (_: string, fn: () => void) => listeners.add(fn),
    removeEventListener: (_: string, fn: () => void) => listeners.delete(fn),
  };
  vi.stubGlobal("matchMedia", () => query);
  return {
    set(next: boolean) {
      query.matches = next;
      for (const fn of listeners) fn();
    },
  };
}

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  unregisterTheme("plugin.neon");
  applyAppearance(DEFAULT_APPEARANCE);
  vi.unstubAllGlobals();
});

describe("theme parameters", () => {
  it("gives defaults per scheme", () => {
    expect(paramDefault(accent, "light")).toBe("#112233");
    expect(paramDefault(accent, "dark")).toBe("#aabbcc");
    expect(paramDefault({ ...accent, default: "#FFF" }, "dark")).toBe("#ffffff");
    expect(paramDefault(size, "dark")).toBe(10);
    expect(paramDefault(glow, "light")).toBe(false);
  });

  it("validates and clamps saved values", () => {
    expect(sanitizeParam(accent, "#F80", "dark")).toBe("#ff8800");
    expect(sanitizeParam(accent, "url(x)", "dark")).toBe("#aabbcc");
    expect(sanitizeParam(accent, 5, "light")).toBe("#112233");
    expect(sanitizeParam(size, 12, "dark")).toBe(12);
    expect(sanitizeParam(size, 500, "dark")).toBe(20);
    expect(sanitizeParam(size, -1, "dark")).toBe(5);
    expect(sanitizeParam(size, Number.NaN, "dark")).toBe(10);
    expect(sanitizeParam(size, "12", "dark")).toBe(10);
    expect(sanitizeParam(shape, "square", "dark")).toBe("square");
    expect(sanitizeParam(shape, "star; } body { display: none", "dark")).toBe("round");
    expect(sanitizeParam(glow, true, "dark")).toBe(true);
    expect(sanitizeParam(glow, "yes", "dark")).toBe(false);
    expect(sanitizeParam(glow, undefined, "dark")).toBe(false);
  });

  it("merges saved values with defaults and ignores unknown keys", () => {
    expect(resolveParams(neon(), { size: 7, old: "x", shape: "nope" }, "dark")).toEqual({
      accent: "#aabbcc",
      size: 7,
      shape: "round",
      glow: false,
    });
    expect(resolveParams(neon(), undefined, "light").accent).toBe("#112233");
  });

  it("turns values into CSS", () => {
    expect(paramCssValue(accent, "#ff8800")).toBe("#ff8800");
    expect(paramCssValue(size, 12)).toBe("12px");
    expect(paramCssValue({ ...size, unit: undefined }, 110)).toBe("110");
    expect(paramCssValue(shape, "square")).toBe("square");
    expect(paramCssValue(glow, true)).toBe("1");
    expect(paramCssValue(glow, false)).toBe("0");
  });

  it("makes properties for every parameter and attributes for selects and toggles", () => {
    expect(paramProperties(neon(), { accent: "#ffffff", glow: true }, "dark")).toEqual({
      vars: {
        "--qk-param-accent": "#ffffff",
        "--qk-param-accent-contrast": "#0e1220",
        "--qk-param-size": "10px",
        "--qk-param-shape": "round",
        "--qk-param-glow": "1",
      },
      attrs: { "data-param-shape": "round", "data-param-glow": "1" },
    });
    // A key that would not make a valid attribute name is skipped.
    const odd = neon({ params: [{ ...glow, key: 'x" onload="' }, size] });
    expect(paramProperties(odd, {}, "dark")).toEqual({
      vars: { "--qk-param-size": "10px" },
      attrs: {},
    });
    expect(paramProperties(neon(), {}, "light").vars["--qk-param-accent-contrast"]).toBe("#ffffff");
  });

  it("sets and resets saved parameters without leaving empty entries", () => {
    const one = withParam(appearance(), "plugin.neon", "size", 12);
    expect(one.params).toEqual({ "plugin.neon": { size: 12 } });
    const two = withParam(withParam(one, "plugin.neon", "glow", true), "other", "a", "b");
    expect(two.params).toEqual({ "plugin.neon": { size: 12, glow: true }, other: { a: "b" } });
    expect(withParam(two, "plugin.neon", "size", undefined).params["plugin.neon"]).toEqual({
      glow: true,
    });
    expect(withParam(one, "plugin.neon", "size", undefined).params).toEqual({});
    expect(withoutParams(two, "plugin.neon").params).toEqual({ other: { a: "b" } });
    // The input is left alone.
    expect(one.params).toEqual({ "plugin.neon": { size: 12 } });
  });

  it("shows texts as written, or through the theme's translation", () => {
    expect(themeText(neon(), "Glow")).toBe("Glow");
    expect(themeText(neon({ text: (key) => `<${key}>` }), "glow")).toBe("<glow>");
  });
});

describe("theme registry", () => {
  it("has the built-in themes from the start, the default one first", () => {
    expect(themes()[0]?.id).toBe("questkeep");
    expect(themes().length).toBeGreaterThanOrEqual(7);
    expect(themes().every((t) => t.origin === "builtin")).toBe(true);
    expect(getTheme("midnight")?.name).toBe("Midnight");
    expect(getTheme("nope")).toBeUndefined();
  });

  it("registers, replaces in place and unregisters", () => {
    const count = themes().length;
    const first = neon();
    const undoFirst = registerTheme(first);
    expect(themes().length).toBe(count + 1);
    expect(getTheme("plugin.neon")).toBe(first);
    const second = neon({ name: "Neon 2" });
    const undoSecond = registerTheme(second);
    expect(themes().length).toBe(count + 1);
    expect(getTheme("plugin.neon")?.name).toBe("Neon 2");
    // The first registration was replaced: taking it back must not remove the new one.
    undoFirst();
    expect(getTheme("plugin.neon")).toBe(second);
    undoSecond();
    expect(getTheme("plugin.neon")).toBeUndefined();
    expect(themes().length).toBe(count);
    unregisterTheme("plugin.neon");
    expect(themes().length).toBe(count);
  });
});

describe("resolving the appearance", () => {
  it("resolves the scheme", () => {
    expect(resolveScheme("system", true)).toBe("dark");
    expect(resolveScheme("system", false)).toBe("light");
    expect(resolveScheme("light", true)).toBe("light");
    expect(resolveScheme("dark", false)).toBe("dark");
  });

  it("falls back to the default theme for an unknown id, with that theme's parameters", () => {
    const resolved = resolveAppearance(
      appearance({
        theme: "plugin.gone",
        scheme: "light",
        params: { "plugin.gone": { accent: "#ff0000" }, questkeep: { corners: "round" } },
      }),
      true,
    );
    expect(resolved.scheme).toBe("light");
    expect(resolved.theme).toBe("questkeep");
    expect(resolved.vars["--qk-param-accent"]).toBe("#3d5fd9");
    expect(resolved.attrs["data-param-corners"]).toBe("round");
    expect(resolveAppearance(appearance(), true, [])).toEqual({
      scheme: "dark",
      theme: "questkeep",
      css: "",
      vars: {},
      attrs: {},
    });
  });
});

describe("applying the appearance", () => {
  it("sets the attributes, the style element and the parameter properties", () => {
    fakeMedia(true);
    registerTheme(neon());
    applyAppearance(
      appearance({
        theme: "plugin.neon",
        scheme: "light",
        params: { "plugin.neon": { size: 8, glow: true } },
      }),
    );
    expect(root.dataset.scheme).toBe("light");
    expect(root.dataset.theme).toBe("plugin.neon");
    expect(effectiveScheme()).toBe("light");
    expect(activeThemeId()).toBe("plugin.neon");
    expect(root.style.getPropertyValue("--qk-param-accent")).toBe("#112233");
    expect(root.style.getPropertyValue("--qk-param-accent-contrast")).toBe("#ffffff");
    expect(root.style.getPropertyValue("--qk-param-size")).toBe("8px");
    expect(root.getAttribute("data-param-glow")).toBe("1");
    expect(root.getAttribute("data-param-shape")).toBe("round");
    expect(styleText()).toContain("#010203");
    // One style element, last in <head> (after the app's stylesheets).
    const late = document.createElement("style");
    document.head.append(late);
    applyAppearance(appearance({ theme: "plugin.neon", scheme: "dark" }));
    expect(document.querySelectorAll("#qk-theme").length).toBe(1);
    expect(document.head.lastElementChild?.id).toBe("qk-theme");
    expect(root.style.getPropertyValue("--qk-param-accent")).toBe("#aabbcc");
    late.remove();
  });

  it("removes the previous theme's properties and attributes", () => {
    fakeMedia(true);
    registerTheme(neon());
    applyAppearance(appearance({ theme: "plugin.neon" }));
    expect(root.style.getPropertyValue("--qk-param-size")).toBe("10px");
    applyAppearance(appearance({ theme: "fjord" }));
    expect(root.dataset.theme).toBe("fjord");
    expect(root.style.getPropertyValue("--qk-param-size")).toBe("");
    expect(root.hasAttribute("data-param-glow")).toBe(false);
    expect(root.style.getPropertyValue("--qk-param-accent")).toBe("#6cc5e8");
  });

  it("shows the default theme for an unknown id until that theme is registered", () => {
    fakeMedia(true);
    applyAppearance(appearance({ theme: "plugin.neon", params: { "plugin.neon": { size: 6 } } }));
    expect(root.dataset.theme).toBe("questkeep");
    expect(activeThemeId()).toBe("questkeep");
    expect(root.getAttribute("data-param-corners")).toBe("default");
    const undo = registerTheme(neon());
    expect(root.dataset.theme).toBe("plugin.neon");
    expect(root.style.getPropertyValue("--qk-param-size")).toBe("6px");
    expect(root.hasAttribute("data-param-corners")).toBe(false);
    // A new definition of the active theme (a live editor) shows right away.
    registerTheme(neon({ css: "/* v2 */" }));
    expect(styleText()).toBe("/* v2 */");
    undo();
    expect(root.dataset.theme).toBe("plugin.neon");
    unregisterTheme("plugin.neon");
    expect(root.dataset.theme).toBe("questkeep");
  });

  it("follows the system while the scheme is System, and only then", () => {
    const media = fakeMedia(false);
    applyAppearance(appearance({ scheme: "system" }));
    expect(root.dataset.scheme).toBe("light");
    expect(root.style.getPropertyValue("--qk-param-accent")).toBe("#3d5fd9");
    media.set(true);
    expect(root.dataset.scheme).toBe("dark");
    expect(effectiveScheme()).toBe("dark");
    expect(root.style.getPropertyValue("--qk-param-accent")).toBe("#7c9cff");
    applyAppearance(appearance({ scheme: "light" }));
    media.set(false);
    media.set(true);
    expect(root.dataset.scheme).toBe("light");
  });

  it("caches what it applied and shows it again before the settings are loaded", () => {
    fakeMedia(true);
    registerTheme(neon());
    applyAppearance(appearance({ theme: "plugin.neon", scheme: "light" }));
    const cached = cachedTheme();
    expect(cached).toMatchObject({ scheme: "light", theme: "plugin.neon" });
    expect(cached?.css).toContain("#010203");

    // A new start: nothing applied yet, the plugin not loaded.
    unregisterTheme("plugin.neon");
    applyResolved({ scheme: "dark", theme: "questkeep", css: "", vars: {}, attrs: {} });
    localStorage.setItem("questkeep.appearance", JSON.stringify(cached));
    applyCachedAppearance();
    expect(root.dataset.scheme).toBe("light");
    expect(root.dataset.theme).toBe("plugin.neon");
    expect(styleText()).toContain("#010203");
    expect(root.style.getPropertyValue("--qk-param-size")).toBe("10px");
    // The real settings replace it, leftovers included.
    applyAppearance(appearance({ scheme: "dark" }));
    expect(root.dataset.theme).toBe("questkeep");
    expect(root.style.getPropertyValue("--qk-param-size")).toBe("");
  });

  it("starts with the default theme in the system's scheme without a usable cache", () => {
    fakeMedia(false);
    for (const broken of [null, "{not json", '{"scheme":"blue"}', '{"scheme":"dark","theme":1}']) {
      if (broken === null) localStorage.clear();
      else localStorage.setItem("questkeep.appearance", broken);
      expect(cachedTheme()).toBeNull();
      applyCachedAppearance();
      expect(root.dataset.scheme).toBe("light");
      expect(root.dataset.theme).toBe("questkeep");
      expect(root.style.getPropertyValue("--qk-param-text-size")).toBe("100");
    }
  });

  it("works when storage is not available", () => {
    fakeMedia(true);
    const setItem = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    const getItem = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    expect(() => applyAppearance(appearance({ theme: "ember" }))).not.toThrow();
    expect(root.dataset.theme).toBe("ember");
    expect(cachedTheme()).toBeNull();
    expect(() => applyCachedAppearance()).not.toThrow();
    setItem.mockRestore();
    getItem.mockRestore();
  });
});
