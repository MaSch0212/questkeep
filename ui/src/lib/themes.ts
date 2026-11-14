/**
 * Themes: the registry (built-in themes; plugin themes are added by pluginThemes.ts, the user's
 * own by userThemeStore.ts), turning a theme's
 * parameters into CSS custom properties, and applying the appearance settings to `<html>`.
 *
 * What the page gets (see theme.css for the variables themes override):
 * - `data-scheme="light|dark"` and `data-theme="<id>"` on `<html>`,
 * - one `<style id="qk-theme">` with the active theme's CSS, after the app's stylesheets,
 * - per parameter `--qk-param-<key>` on `<html>`; colors also get `--qk-param-<key>-contrast`
 *   (dark or white text that reads on it); selects and toggles also `data-param-<key>`.
 */
import { createSignal } from "solid-js";
import type { Appearance } from "../bindings/Appearance";
import type { ColorScheme } from "../bindings/ColorScheme";
import { BUILTIN_THEMES } from "../themes";
import { contrastColor, normalizeHex } from "./color";

interface ParamBase {
  /** Letters, digits and dashes; becomes `--qk-param-<key>`. */
  key: string;
  /** Shown as it is, or looked up through the theme's `text` when it has one. */
  label: string;
  /** Shown in the collapsed "Advanced" part of the settings. */
  advanced?: boolean;
  hint?: string;
}

export type ThemeParam =
  | (ParamBase & { type: "color"; default: string | { light: string; dark: string } })
  | (ParamBase & {
      type: "number";
      default: number;
      min: number;
      max: number;
      step?: number;
      /** Appended to the value in CSS and in the settings (`px`, `%`). */
      unit?: string;
    })
  | (ParamBase & {
      type: "select";
      default: string;
      options: { value: string; label: string }[];
    })
  | (ParamBase & { type: "toggle"; default: boolean });

export interface ThemeDef {
  id: string;
  /** Shown as it is (theme names are not translated). */
  name: string;
  description?: string;
  author?: string;
  /**
   * CSS applied while the theme is active. Rules are written against
   * `:root[data-theme="<id>"]`, optionally combined with `[data-scheme="light|dark"]` and
   * `[data-param-<key>="<value>"]`.
   */
  css: string;
  params: ThemeParam[];
  /** Where it comes from. */
  origin: "builtin" | "plugin" | "user";
  /** The plugin a plugin theme comes from. */
  plugin?: { id: string; name: string };
  /** Preview swatches per scheme for the picker: background, surface, text, accent. */
  preview?: { light: string[]; dark: string[] };
  /**
   * Translates the description and the labels, hints and option labels of the parameters
   * (built-in themes pass message keys). Without it they are shown as written.
   */
  text?: (key: string) => string;
}

export type Scheme = "light" | "dark";
export type ParamValue = string | number | boolean;
/** Saved parameter values of one theme. */
export type SavedParams = Record<string, ParamValue | undefined>;

/** What is on `<html>` for a theme; also what is cached for the next start. */
export interface AppliedTheme {
  scheme: Scheme;
  /** The theme shown (the default one when the chosen theme is not registered). */
  theme: string;
  css: string;
  /** Custom properties by name (`--qk-param-accent`). */
  vars: Record<string, string>;
  /** Attributes by name (`data-param-corners`). */
  attrs: Record<string, string>;
}

export const DEFAULT_THEME_ID = "questkeep";
export const DEFAULT_APPEARANCE: Appearance = {
  theme: DEFAULT_THEME_ID,
  scheme: "system",
  params: {},
};

// ---------------------------------------------------------------------------------------------
// Registry

const [themes, setThemes] = createSignal<ThemeDef[]>([...BUILTIN_THEMES]);

/** Every registered theme, built-in ones first, then in the order they were registered. */
export { themes };

export function getTheme(id: string): ThemeDef | undefined {
  return themes().find((theme) => theme.id === id);
}

/**
 * Adds a theme, or replaces the one with the same id (in place, so the picker keeps its order).
 * The page follows right away when it is the chosen theme. Returns a function that takes this
 * registration back (a no-op once another definition replaced it).
 */
export function registerTheme(def: ThemeDef): () => void {
  setThemes((all) =>
    all.some((t) => t.id === def.id) ? all.map((t) => (t.id === def.id ? def : t)) : [...all, def],
  );
  reapply();
  return () => {
    if (getTheme(def.id) === def) unregisterTheme(def.id);
  };
}

/** Removes a theme. While it is the chosen one the default theme shows; the choice is kept. */
export function unregisterTheme(id: string): void {
  if (!getTheme(id)) return;
  setThemes((all) => all.filter((t) => t.id !== id));
  reapply();
}

/** A theme's text for display: through its `text` when it has one. */
export function themeText(theme: ThemeDef, text: string): string {
  return theme.text ? theme.text(text) : text;
}

// ---------------------------------------------------------------------------------------------
// Parameters (pure)

/** A parameter's default in a scheme. Colors come back as `#rrggbb`. */
export function paramDefault(param: ThemeParam, scheme: Scheme): ParamValue {
  if (param.type !== "color") return param.default;
  const value = typeof param.default === "string" ? param.default : param.default[scheme];
  return normalizeHex(value) ?? value;
}

/**
 * A saved value made valid for its parameter: colors as `#rrggbb`, numbers within min and max,
 * selects one of the options. Anything else (also a missing value) gives the default.
 */
export function sanitizeParam(param: ThemeParam, saved: unknown, scheme: Scheme): ParamValue {
  switch (param.type) {
    case "color": {
      const color = typeof saved === "string" ? normalizeHex(saved) : null;
      return color ?? paramDefault(param, scheme);
    }
    case "number":
      return typeof saved === "number" && Number.isFinite(saved)
        ? Math.min(param.max, Math.max(param.min, saved))
        : param.default;
    case "select":
      return typeof saved === "string" && param.options.some((o) => o.value === saved)
        ? saved
        : param.default;
    case "toggle":
      return typeof saved === "boolean" ? saved : param.default;
  }
}

/** Every parameter of a theme with its saved value, or the default where none (valid) is saved. */
export function resolveParams(
  theme: ThemeDef,
  saved: SavedParams | undefined,
  scheme: Scheme,
): Record<string, ParamValue> {
  return Object.fromEntries(
    theme.params.map((param) => [param.key, sanitizeParam(param, saved?.[param.key], scheme)]),
  );
}

/** A parameter value as CSS: the color, number + unit, the option, `1`/`0` for a toggle. */
export function paramCssValue(param: ThemeParam, value: ParamValue): string {
  switch (param.type) {
    case "number":
      return `${String(value)}${param.unit ?? ""}`;
    case "toggle":
      return value === true ? "1" : "0";
    default:
      return String(value);
  }
}

/** What a parameter key may be: letters, digits and dashes. */
const PARAM_KEY = /^[a-z0-9][a-z0-9-]*$/i;

export const paramVarName = (key: string) => `--qk-param-${key}`;
export const paramAttrName = (key: string) => `data-param-${key}`;

/** The custom properties and attributes `<html>` gets for a theme's parameters. */
export function paramProperties(
  theme: ThemeDef,
  saved: SavedParams | undefined,
  scheme: Scheme,
): Pick<AppliedTheme, "vars" | "attrs"> {
  const values = resolveParams(theme, saved, scheme);
  const vars: Record<string, string> = {};
  const attrs: Record<string, string> = {};
  for (const param of theme.params) {
    // A key that cannot be part of a property or attribute name is left out, not an error.
    if (!PARAM_KEY.test(param.key)) continue;
    const css = paramCssValue(param, values[param.key] as ParamValue);
    vars[paramVarName(param.key)] = css;
    if (param.type === "color") vars[`${paramVarName(param.key)}-contrast`] = contrastColor(css);
    if (param.type === "select" || param.type === "toggle") attrs[paramAttrName(param.key)] = css;
  }
  return { vars, attrs };
}

/** `appearance` with one parameter of a theme set, or back on its default (`undefined`). */
export function withParam(
  appearance: Appearance,
  themeId: string,
  key: string,
  value: ParamValue | undefined,
): Appearance {
  const { [key]: _old, ...rest } = appearance.params[themeId] ?? {};
  const next = value === undefined ? rest : { ...rest, [key]: value };
  const { [themeId]: _theme, ...others } = appearance.params;
  return {
    ...appearance,
    params: Object.keys(next).length === 0 ? others : { ...others, [themeId]: next },
  };
}

/** `appearance` with every parameter of a theme back on its default. */
export function withoutParams(appearance: Appearance, themeId: string): Appearance {
  const { [themeId]: _theme, ...others } = appearance.params;
  return { ...appearance, params: others };
}

// ---------------------------------------------------------------------------------------------
// Resolving and applying

/** Light or dark for a scheme setting; "system" follows the system's preference. */
export function resolveScheme(setting: ColorScheme, systemDark: boolean): Scheme {
  if (setting === "system") return systemDark ? "dark" : "light";
  return setting;
}

/** What `<html>` should carry for the settings, with the themes registered right now. */
export function resolveAppearance(
  appearance: Appearance,
  systemDark: boolean,
  registered: ThemeDef[] = themes(),
): AppliedTheme {
  const scheme = resolveScheme(appearance.scheme, systemDark);
  const theme =
    registered.find((t) => t.id === appearance.theme) ??
    registered.find((t) => t.id === DEFAULT_THEME_ID);
  if (!theme) return { scheme, theme: DEFAULT_THEME_ID, css: "", vars: {}, attrs: {} };
  return {
    scheme,
    theme: theme.id,
    css: theme.css,
    ...paramProperties(theme, appearance.params[theme.id], scheme),
  };
}

const STYLE_ID = "qk-theme";
/** Where the last applied theme is kept, so the first paint already has it. */
const STORAGE_KEY = "questkeep.appearance";

const darkQuery = (): MediaQueryList | null =>
  typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-color-scheme: dark)")
    : null;

/** Whether the system asks for dark; dark too where that is unknown (the app's first look). */
const systemDark = () => darkQuery()?.matches ?? true;

const [effectiveScheme, setEffectiveScheme] = createSignal<Scheme>("dark");
const [activeThemeId, setActiveThemeId] = createSignal(DEFAULT_THEME_ID);

/** The scheme on the page now ("system" resolved). */
export { effectiveScheme };
/** The id of the theme on the page now (the default one while the chosen theme is missing). */
export { activeThemeId };

/** The settings last applied; null until `applyAppearance` ran (a cached theme may show). */
let current: Appearance | null = null;
/** Shown instead of `current` while set (see `previewAppearance`). */
let preview: ((applied: Appearance) => Appearance) | null = null;
/** The properties and attributes this module put on `<html>`, to take them off again. */
let appliedVars: string[] = [];
let appliedAttrs: string[] = [];
/** The media query whose changes are followed (kept, so it is not collected), and its source. */
let listened: MediaQueryList | null = null;
let listenedSource: typeof window.matchMedia | undefined;

/** Puts a resolved theme on the page, replacing whatever this module applied before. */
export function applyResolved(applied: AppliedTheme): void {
  setEffectiveScheme(applied.scheme);
  setActiveThemeId(applied.theme);
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.dataset.scheme = applied.scheme;
  root.dataset.theme = applied.theme;
  for (const name of appliedVars) if (!(name in applied.vars)) root.style.removeProperty(name);
  for (const name of appliedAttrs) if (!(name in applied.attrs)) root.removeAttribute(name);
  for (const [name, value] of Object.entries(applied.vars)) root.style.setProperty(name, value);
  for (const [name, value] of Object.entries(applied.attrs)) root.setAttribute(name, value);
  appliedVars = Object.keys(applied.vars);
  appliedAttrs = Object.keys(applied.attrs);

  let style = document.getElementById(STYLE_ID) as HTMLStyleElement | null;
  if (!style) {
    style = document.createElement("style");
    style.id = STYLE_ID;
  }
  if (style.textContent !== applied.css) style.textContent = applied.css;
  // Last in <head>, so theme rules win over the app's stylesheets at equal specificity.
  if (document.head.lastElementChild !== style) document.head.append(style);
}

function reapply(): void {
  const shown = preview ? preview(current ?? DEFAULT_APPEARANCE) : current;
  if (!shown) return;
  const applied = resolveAppearance(shown, systemDark());
  applyResolved(applied);
  // A preview is not what the next start should show.
  if (preview) return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(applied));
  } catch {
    // Without storage the next start shows the default theme until the settings are loaded.
  }
}

/** Follows the system's light/dark switch (once per `matchMedia`, which only tests replace). */
function listen(): void {
  const source = typeof window === "undefined" ? undefined : window.matchMedia;
  if (source === listenedSource) return;
  listened?.removeEventListener?.("change", reapply);
  listenedSource = source;
  listened = darkQuery();
  // Only "system" depends on it, and reapplying the same thing is harmless.
  listened?.addEventListener?.("change", reapply);
}

/**
 * Applies the appearance settings to the page and remembers them: the page follows when the
 * system switches between light and dark ("system" scheme), and when the chosen theme is
 * registered, replaced or removed later.
 */
export function applyAppearance(appearance: Appearance | null | undefined): void {
  // Settings from a source that knows no appearance yet count as the defaults.
  current = { ...DEFAULT_APPEARANCE, ...appearance };
  listen();
  reapply();
}

/**
 * Shows an appearance without making it the applied one (the theme editor's live preview). The
 * function gets the applied settings and returns what to show instead; it runs again whenever
 * the page is re-applied, so settings saved meanwhile (a parameter, say) still come through.
 * `null` ends the preview and shows the applied settings again.
 */
export function previewAppearance(make: ((applied: Appearance) => Appearance) | null): void {
  preview = make;
  listen();
  reapply();
}

/** The settings last given to `applyAppearance` (what shows when no preview is on). */
export function appliedAppearance(): Appearance | null {
  return current;
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const isStrings = (value: unknown): value is Record<string, string> =>
  isRecord(value) && Object.values(value).every((v) => typeof v === "string");

/** The theme cached by the last run, if it is there and well-formed. */
export function cachedTheme(): AppliedTheme | null {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null");
    if (!isRecord(parsed)) return null;
    const { scheme, theme, css, vars, attrs } = parsed;
    if (scheme !== "light" && scheme !== "dark") return null;
    if (typeof theme !== "string" || typeof css !== "string") return null;
    if (!isStrings(vars) || !isStrings(attrs)) return null;
    return { scheme, theme, css, vars, attrs };
  } catch {
    return null;
  }
}

/**
 * Called once before the first render: shows the theme the last run ended with (no flash of the
 * default look), or the default theme in the system's scheme. The loaded settings follow through
 * `applyAppearance`.
 */
export function applyCachedAppearance(): void {
  const cached = cachedTheme();
  if (cached) applyResolved(cached);
  else applyResolved(resolveAppearance(DEFAULT_APPEARANCE, systemDark()));
}
