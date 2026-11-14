import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { LOCALES, setLanguage } from "../i18n";
import { contrastRatio, toHex, type Rgba } from "../lib/color";
import { resolveColor, rootVars } from "../lib/cssVars";
import { contrastFailures } from "../lib/themeContrast";
import { paramProperties, themeText, type Scheme, type ThemeDef } from "../lib/themes";
import baseCss from "../theme.css?raw";
import { BUILTIN_THEMES } from ".";

const read = (path: string) =>
  readFileSync(new URL(path, import.meta.url), "utf8").replace(/\r\n/g, "\n");
const THEMES: ThemeDef[] = BUILTIN_THEMES;

const SCHEMES: Scheme[] = ["dark", "light"];

/** The variables `<html>` has for a theme in a scheme, with the given saved parameters. */
function varsOf(theme: ThemeDef, scheme: Scheme, saved: Record<string, string | number> = {}) {
  const { vars, attrs } = paramProperties(theme, saved, scheme);
  return rootVars(
    [baseCss, theme.css],
    { "data-scheme": scheme, "data-theme": theme.id, ...attrs },
    vars,
  );
}

const cases = THEMES.flatMap((theme) =>
  SCHEMES.map((scheme) => ({ theme, scheme, name: `${theme.id} ${scheme}` })),
);

describe("built-in themes", () => {
  it("has the default theme first and at least six more, with unique ids", () => {
    expect(BUILTIN_THEMES[0]?.id).toBe("questkeep");
    expect(BUILTIN_THEMES.length).toBeGreaterThanOrEqual(7);
    expect(new Set(BUILTIN_THEMES.map((t) => t.id)).size).toBe(BUILTIN_THEMES.length);
  });

  it.each(THEMES)("$id is wired to its own CSS file", (theme) => {
    expect(theme.css.trim()).not.toBe("");
    expect(theme.css.replace(/\r\n/g, "\n")).toBe(read(`./${theme.id}.css`));
    expect(baseCss.replace(/\r\n/g, "\n")).toBe(read("../theme.css"));
  });

  it.each(THEMES)("$id starts with the accent and keeps extras few", (theme) => {
    expect(theme.params[0]).toMatchObject({ key: "accent", type: "color" });
    expect(theme.params.filter((p) => !p.advanced).length).toBeLessThanOrEqual(4);
    expect(new Set(theme.params.map((p) => p.key)).size).toBe(theme.params.length);
    for (const param of theme.params) expect(param.key).toMatch(/^[a-z][a-z0-9-]*$/);
    // Every rule is scoped to the theme.
    const selectors = theme.css
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/\{[^{}]*\}/g, "\n")
      .replace(/@supports[^{]*\{|\}/g, "\n")
      .split(/[\n,]/)
      .map((s) => s.trim())
      .filter((s) => s !== "");
    expect(selectors.filter((s) => !s.startsWith(`:root[data-theme="${theme.id}"]`))).toEqual([]);
  });

  it.each(THEMES)("$id has translated texts in every language", (theme) => {
    const texts = [
      theme.description ?? "",
      ...theme.params.flatMap((p) => [
        p.label,
        p.hint ?? "",
        ...(p.type === "select" ? p.options.map((o) => o.label) : []),
      ]),
    ].filter((t) => t !== "");
    for (const locale of LOCALES) {
      setLanguage(locale);
      for (const key of texts) {
        const shown = themeText(theme, key);
        expect([locale, key, shown]).not.toEqual([locale, key, key]);
        expect(shown.trim()).not.toBe("");
      }
    }
  });

  it.each(cases)("$name: text and accent are readable", ({ theme, scheme }) => {
    const vars = varsOf(theme, scheme);
    expect(contrastFailures(vars)).toEqual([]);
  });

  it.each(cases)("$name: the preview shows the theme's colors", ({ theme, scheme }) => {
    const vars = varsOf(theme, scheme);
    const hex = (name: string) => toHex(resolveColor(`--qk-color-${name}`, vars) as Rgba);
    expect(theme.preview?.[scheme]).toEqual([
      hex("bg"),
      hex("surface"),
      hex("text"),
      hex("accent"),
    ]);
  });

  it.each(cases)("$name: every color variable resolves", ({ theme, scheme }) => {
    const vars = varsOf(theme, scheme);
    const colors = [...vars.keys()].filter(
      (name) => name.startsWith("--qk-color-") || name.startsWith("--qk-filter-accent-"),
    );
    expect(colors.length).toBeGreaterThan(60);
    const unreadable = colors.filter((name) => {
      try {
        resolveColor(name, vars);
        return false;
      } catch {
        return true;
      }
    });
    expect(unreadable).toEqual([]);
  });

  it("a white or black accent still gets readable text on it", () => {
    const failures: string[] = [];
    for (const { theme, scheme } of cases) {
      for (const accent of ["#ffffff", "#000000", "#ff0000", "#00ff00", "#808080"]) {
        const vars = varsOf(theme, scheme, { accent });
        const ratio = contrastRatio(
          resolveColor("--qk-color-accent-contrast", vars),
          resolveColor("--qk-color-accent", vars),
        );
        if (ratio < 4.5) failures.push(`${theme.id} ${scheme} ${accent}: ${ratio.toFixed(2)}`);
      }
    }
    expect(failures).toEqual([]);
  });

  it("the default theme's parameters reach the variables", () => {
    const theme = THEMES[0] as ThemeDef;
    const plain = varsOf(theme, "dark");
    expect(toHex(resolveColor("--qk-color-bg", plain))).toBe("#111317");
    expect(plain.get("--qk-font-scale")).toBe("calc(var(--qk-param-text-size, 100) / 100)");
    expect(plain.get("--qk-radius-scale")).toBe("1");
    const custom = varsOf(theme, "dark", {
      accent: "#ff8800",
      corners: "round",
      density: "compact",
      "tint-strength": 20,
      tint: "#ff0000",
      font: "serif",
    });
    expect(toHex(resolveColor("--qk-color-accent", custom))).toBe("#ff8800");
    expect(toHex(resolveColor("--qk-color-heat-4", custom))).toBe("#ff8800");
    expect(custom.get("--qk-radius-scale")).toBe("1.6");
    expect(custom.get("--qk-space-scale")).toBe("0.8");
    expect(custom.get("--qk-font-family")).toMatch(/^Georgia/);
    const tinted = resolveColor("--qk-color-bg", custom);
    expect(tinted.r).toBeGreaterThan(tinted.b);
    expect(toHex(tinted)).not.toBe("#111317");
  });
});
