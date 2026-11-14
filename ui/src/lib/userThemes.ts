/**
 * User themes (made in the theme editor): turning one into a `ThemeDef` with generated CSS, the
 * catalogue of variables the editor offers, and small helpers for editing overrides. All pure.
 *
 * Generated CSS, in this order:
 * 1. the base theme's CSS, re-targeted to the user theme's id (so its parameters keep working),
 * 2. the overrides: for both schemes, then light, then dark, with a selector that outranks every
 *    rule of a base theme,
 * 3. the free CSS, nested in `:root[data-theme="<id>"] { … }` so it only applies while the theme
 *    is active (authors write plain selectors; `&[data-scheme="dark"] .foo {}` targets one scheme).
 */
import type { ExportedTheme } from "../bindings/ExportedTheme";
import type { ThemeOverrides } from "../bindings/ThemeOverrides";
import type { UserTheme } from "../bindings/UserTheme";
import baseCss from "../theme.css?raw";
import { toHex } from "./color";
import { resolveColor, rootVars } from "./cssVars";
import {
  paramProperties,
  themeText,
  type ParamValue,
  type Scheme,
  type ThemeDef,
  type ThemeParam,
} from "./themes";

export const USER_THEME_PREFIX = "user-";
const USER_THEME_ID = /^user-[a-z0-9-]{1,48}$/;
const VARIABLE_NAME = /^--qk-[a-z0-9-]+$/;
const MAX_VALUE_LENGTH = 512;
export const MAX_NAME_LENGTH = 80;

export const isUserThemeId = (id: string) => USER_THEME_ID.test(id);

/** Line breaks, tabs and other control characters. */
const isControl = (char: string) => char.charCodeAt(0) < 0x20 || char.charCodeAt(0) === 0x7f;

/** Whether a variable name can be overridden (`--qk-` and lower-case letters, digits, dashes). */
export const isVariableName = (name: string) => VARIABLE_NAME.test(name) && name.length <= 96;

/**
 * Whether a value can be saved as an override: not empty, one line, and nothing that could end
 * its declaration (`{`, `}`, `;`, `</`). The core checks the same before writing a theme.
 */
export function isOverrideValue(value: string): boolean {
  return (
    value.trim() !== "" &&
    value.length <= MAX_VALUE_LENGTH &&
    !/[{};]/.test(value) &&
    ![...value].some(isControl) &&
    !value.includes("</")
  );
}

/** A theme id for a name: `user-<kebab-name>`, with `-2`, `-3`… while that id is taken. */
export function userThemeId(name: string, taken: readonly string[]): string {
  const slug =
    name
      .normalize("NFKD")
      .replace(/[̀-ͯ]/g, "")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 40)
      .replace(/-+$/, "") || "theme";
  let id = `${USER_THEME_PREFIX}${slug}`;
  for (let n = 2; taken.includes(id); n++) id = `${USER_THEME_PREFIX}${slug}-${n}`;
  return id;
}

/** The id of the plugin (and its theme) a user theme is exported as. */
export function pluginThemeId(id: string): string {
  return `${id.startsWith(USER_THEME_PREFIX) ? id.slice(USER_THEME_PREFIX.length) : id}-theme`;
}

/** A name that is not in `taken`: the name itself, else with " 2", " 3"… */
export function uniqueName(name: string, taken: readonly string[]): string {
  let candidate = name;
  for (let n = 2; taken.includes(candidate); n++) candidate = `${name} ${n}`;
  return candidate;
}

export const emptyOverrides = (): ThemeOverrides => ({ both: {}, light: {}, dark: {} });

// ---------------------------------------------------------------------------------------------
// Overrides

/** Where an override of a variable comes from for a scheme, if there is one. */
export function overrideSource(
  overrides: ThemeOverrides,
  name: string,
  scheme: Scheme,
): Scheme | "both" | null {
  if (overrides[scheme][name] !== undefined) return scheme;
  return overrides.both[name] !== undefined ? "both" : null;
}

/** The override of a variable in a scheme: that scheme's own, else the one for both. */
export function overrideValue(
  overrides: ThemeOverrides,
  name: string,
  scheme: Scheme,
): string | undefined {
  return overrides[scheme][name] ?? overrides.both[name];
}

const without = (map: Record<string, string>, name: string) => {
  const { [name]: _removed, ...rest } = map;
  return rest;
};

/**
 * Overrides with a variable set for one scheme or for both. Setting it for both drops the edited
 * scheme's own value (it would hide the new one); the other scheme's own value stays.
 */
export function withOverride(
  overrides: ThemeOverrides,
  name: string,
  value: string,
  target: Scheme | "both",
  scheme: Scheme,
): ThemeOverrides {
  if (target === "both") {
    return {
      ...overrides,
      both: { ...overrides.both, [name]: value },
      [scheme]: without(overrides[scheme], name),
    };
  }
  return { ...overrides, [target]: { ...overrides[target], [name]: value } };
}

/** Overrides without what a variable shows in `scheme`: that scheme's value and the shared one. */
export function withoutOverride(
  overrides: ThemeOverrides,
  name: string,
  scheme: Scheme,
): ThemeOverrides {
  return {
    ...overrides,
    both: without(overrides.both, name),
    [scheme]: without(overrides[scheme], name),
  };
}

/** How many variables a theme overrides (counted once per variable). */
export function overrideCount(overrides: ThemeOverrides): number {
  return new Set([
    ...Object.keys(overrides.both),
    ...Object.keys(overrides.light),
    ...Object.keys(overrides.dark),
  ]).size;
}

/** Whether the theme sets the accent itself in a scheme (the accent parameter then does nothing). */
export function overridesAccent(overrides: ThemeOverrides, scheme: Scheme): boolean {
  return overrideValue(overrides, "--qk-color-accent", scheme) !== undefined;
}

// ---------------------------------------------------------------------------------------------
// CSS generation

/** A theme's CSS with every `[data-theme="<from>"]` pointing at another theme id. */
export function retargetCss(css: string, from: string, to: string): string {
  return css.split(`[data-theme="${from}"]`).join(`[data-theme="${to}"]`);
}

/**
 * Repeated so overrides outrank any rule of a base theme (theme + scheme + a parameter), whatever
 * the order.
 */
const BOOST = "[data-theme][data-theme][data-theme]";

function block(selector: string, values: Record<string, string>): string {
  const lines = Object.entries(values)
    .filter(([name, value]) => isVariableName(name) && isOverrideValue(value))
    .toSorted(([a], [b]) => a.localeCompare(b))
    .map(([name, value]) => `  ${name}: ${value.trim()};`);
  return lines.length === 0 ? "" : `${selector} {\n${lines.join("\n")}\n}\n`;
}

/** The override rules of a theme: both schemes, then light, then dark. */
export function overridesCss(id: string, overrides: ThemeOverrides): string {
  const root = `:root[data-theme="${id}"]${BOOST}`;
  return (
    block(root, overrides.both) +
    block(`${root}[data-scheme="light"]`, overrides.light) +
    block(`${root}[data-scheme="dark"]`, overrides.dark)
  );
}

/**
 * Whether free CSS keeps its braces in order: a `}` too many would end the wrapper it is nested
 * in and let the rest apply to every theme. (Braces in comments and strings are not counted.)
 */
export function hasBalancedBraces(css: string): boolean {
  const code = css
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/"(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'/g, "");
  let depth = 0;
  for (const c of code) {
    if (c === "{") depth++;
    else if (c === "}" && --depth < 0) return false;
  }
  return true;
}

/** Free CSS nested in the theme's selector; nothing for blank CSS or CSS with a stray `}`. */
export function scopedCss(id: string, css: string): string {
  if (css.trim() === "" || !hasBalancedBraces(css)) return "";
  return `:root[data-theme="${id}"] {\n${css.trim()}\n}\n`;
}

/** The generated CSS without the free CSS: what decides the theme's variables. */
function variablesCss(theme: UserTheme, base: ThemeDef | undefined): string {
  const inherited = base ? retargetCss(base.css, base.id, theme.id).trim() : "";
  return `${inherited === "" ? "" : `${inherited}\n`}${overridesCss(theme.id, theme.overrides)}`;
}

/** The variables `<html>` has with a theme's CSS and parameters in a scheme. */
export function themeVars(
  css: string,
  id: string,
  params: ThemeParam[],
  scheme: Scheme,
  saved?: Record<string, ParamValue | undefined>,
): Map<string, string> {
  const { vars, attrs } = paramProperties(
    { id, name: id, css, params, origin: "user" },
    saved,
    scheme,
  );
  return rootVars([baseCss, css], { "data-scheme": scheme, "data-theme": id, ...attrs }, vars);
}

/**
 * The variables of a user theme in a scheme (base theme, parameters and overrides; the free CSS
 * is left out). With `overrides: false`, those of its base alone: what a reset goes back to.
 */
export function userThemeVars(
  theme: UserTheme,
  base: ThemeDef | undefined,
  scheme: Scheme,
  options: { overrides?: boolean; saved?: Record<string, ParamValue | undefined> } = {},
): Map<string, string> {
  const shown = options.overrides === false ? { ...theme, overrides: emptyOverrides() } : theme;
  return themeVars(variablesCss(shown, base), theme.id, base?.params ?? [], scheme, options.saved);
}

/** A color variable as `#rrggbb(aa)`; null when it is not a color this app can compute. */
export function resolvedHex(name: string, vars: Map<string, string>): string | null {
  try {
    return toHex(resolveColor(name, vars));
  } catch {
    return null;
  }
}

const PREVIEW_VARIABLES = [
  "--qk-color-bg",
  "--qk-color-surface",
  "--qk-color-text",
  "--qk-color-accent",
];

/**
 * The picker's swatches (background, surface, text, accent) of a theme's CSS with its parameters
 * on their defaults. A color that cannot be computed comes from `fallback`, else it is gray.
 */
export function previewSwatches(
  css: string,
  id: string,
  params: ThemeParam[],
  fallback?: ThemeDef["preview"],
): NonNullable<ThemeDef["preview"]> {
  const swatches = (scheme: Scheme) => {
    const vars = themeVars(css, id, params, scheme);
    return PREVIEW_VARIABLES.map(
      (name, i) => resolvedHex(name, vars) ?? fallback?.[scheme][i] ?? "#808080",
    );
  };
  return { light: swatches("light"), dark: swatches("dark") };
}

const previewOf = (css: string, id: string, base: ThemeDef | undefined) =>
  previewSwatches(css, id, base?.params ?? [], base?.preview);

/**
 * A user theme as a registrable theme. `base` is the theme it derives from (the caller passes the
 * default theme when that one is missing): its CSS and parameters come first.
 */
export function userThemeDef(theme: UserTheme, base: ThemeDef | undefined): ThemeDef {
  const variables = variablesCss(theme, base);
  return {
    id: theme.id,
    name: theme.name,
    css: variables + scopedCss(theme.id, theme.css),
    params: base?.params ?? [],
    origin: "user",
    preview: previewOf(variables, theme.id, base),
    text: base?.text,
  };
}

/** A parameter with its texts as shown now, for a plugin that has no translations. */
function literalParam(param: ThemeParam, base: ThemeDef): ThemeParam {
  const text = (value: string) => themeText(base, value);
  const common = {
    label: text(param.label),
    ...(param.hint === undefined ? {} : { hint: text(param.hint) }),
  };
  return param.type === "select"
    ? {
        ...param,
        ...common,
        options: param.options.map((o) => ({ value: o.value, label: text(o.label) })),
      }
    : { ...param, ...common };
}

/** What `export_user_theme` needs: the theme generated against its plugin's theme id. */
export function exportedTheme(theme: UserTheme, base: ThemeDef | undefined): ExportedTheme {
  const def = userThemeDef({ ...theme, id: pluginThemeId(theme.id) }, base);
  return {
    css: def.css,
    params: base ? def.params.map((param) => literalParam(param, base)) : [],
    preview: def.preview ?? null,
  };
}

// ---------------------------------------------------------------------------------------------
// The catalogue of variables

export type VariableGroup =
  | "surfaces"
  | "text"
  | "accent"
  | "status"
  | "charts"
  | "cards"
  | "typography"
  | "shape"
  | "shadows"
  | "motion"
  | "sizes";

/** The groups in the order the editor shows them; `sizes` is the advanced one. */
export const VARIABLE_GROUPS: VariableGroup[] = [
  "surfaces",
  "text",
  "accent",
  "status",
  "charts",
  "cards",
  "typography",
  "shape",
  "shadows",
  "motion",
  "sizes",
];

export type VariableKind = "color" | "length" | "number" | "font" | "shadow" | "other";

export interface ThemeVariable {
  /** With the `--qk-` prefix. */
  name: string;
  /** The name made readable ("Surface hover"); not translated. */
  label: string;
  group: VariableGroup;
  kind: VariableKind;
  /** What theme.css declares per scheme (may be a `calc()`, `var()` or `color-mix()`). */
  base: Record<Scheme, string>;
}

const WORDS: Record<string, string> = { bg: "background", ttb: "time to beat", kpi: "KPI" };

/** `--qk-color-surface-hover` → "Surface hover". */
export function humanizeVariable(name: string): string {
  const words = name
    .replace(/^--qk-/, "")
    .replace(/^color-/, "")
    .split("-")
    .filter((word) => word !== "")
    .map((word) => WORDS[word] ?? word);
  const text = words.join(" ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

export function variableGroup(name: string): VariableGroup {
  const rest = name.replace(/^--qk-/, "");
  if (rest.startsWith("color-")) {
    const color = rest.slice("color-".length);
    if (/^(accent|group-band|edited-border)/.test(color)) return "accent";
    if (color.startsWith("text")) return "text";
    if (/^(bg|surface|border|scrollbar|cursor)/.test(color)) return "surfaces";
    if (/^(danger|success|warn|star|running|play|close-hover)/.test(color)) return "status";
    if (/^(series|heat|drive|ttb)/.test(color)) return "charts";
    return "cards";
  }
  if (/^(filter-accent|accent-soft-mix)/.test(rest)) return "accent";
  if (/^(placeholder-|tile-missing-filter|mask-solid)/.test(rest)) return "cards";
  if (/^(radius-|space-|focus-ring|tile-radius|game-icon-radius)|-scale$/.test(rest)) {
    return "shape";
  }
  if (/^(font-|line-height|letter-spacing)/.test(rest)) return "typography";
  if (rest.startsWith("shadow-")) return "shadows";
  if (/^(duration-|easing)|-duration$/.test(rest)) return "motion";
  return "sizes";
}

const LENGTH = /^-?\d*\.?\d+(px|em|rem|%|vh|vw|ms|s|deg)$/;
const NUMBER = /^-?\d*\.?\d+$/;

export function variableKind(name: string, value: string): VariableKind {
  if (name.startsWith("--qk-color-") || name.startsWith("--qk-filter-accent")) return "color";
  if (name.startsWith("--qk-shadow-")) return "shadow";
  if (name === "--qk-font-family" || name === "--qk-font-mono") return "font";
  if (NUMBER.test(value)) return "number";
  if (LENGTH.test(value) || /^--qk-(space|radius|font-size)-/.test(name)) return "length";
  return "other";
}

/**
 * Every variable of theme.css in the order it is written there, with its group, kind and what
 * the base theme declares in light and dark.
 */
export function themeVariables(css: string = baseCss): ThemeVariable[] {
  const dark = rootVars([css], { "data-scheme": "dark" });
  const light = rootVars([css], { "data-scheme": "light" });
  const names = new Set(
    [...css.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/(--qk-[a-z0-9-]+)\s*:/g)].map(
      (match) => match[1] as string,
    ),
  );
  return [...names]
    .filter((name) => dark.has(name))
    .map((name) => {
      const value = dark.get(name) as string;
      return {
        name,
        label: humanizeVariable(name),
        group: variableGroup(name),
        kind: variableKind(name, value),
        base: { dark: value, light: light.get(name) ?? value },
      };
    });
}

/** The variables whose name or label contains every word of the search text. */
export function filterVariables(variables: ThemeVariable[], search: string): ThemeVariable[] {
  const words = search.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return variables;
  return variables.filter((variable) => {
    const text = `${variable.name} ${variable.label}`.toLowerCase();
    return words.every((word) => text.includes(word));
  });
}

/** Whether changes to a variable go to one scheme by default (colors and shadows) or to both. */
export const isPerScheme = (variable: ThemeVariable) =>
  variable.kind === "color" || variable.kind === "shadow";

const STEPPABLE = /^(-?\d*\.?\d+)([a-z%]*)$/;

/**
 * A length or number one step up or down (ArrowUp/ArrowDown): whole steps, tenths for values
 * with a decimal point or in em/rem/s; ten times that with `big`. Null when the value is not a
 * plain number with an optional unit.
 */
export function stepValue(value: string, direction: 1 | -1, big = false): string | null {
  const match = STEPPABLE.exec(value.trim());
  if (!match) return null;
  const [, digits = "", unit = ""] = match;
  const fine = digits.includes(".") || ["em", "rem", "s"].includes(unit);
  const step = (fine ? 0.1 : 1) * (big ? 10 : 1);
  const next = Math.round((Number(digits) + direction * step) * 1000) / 1000;
  return `${next}${unit}`;
}
