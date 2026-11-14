/**
 * Themes that plugins bring: reading the `themes` entries of a plugin's manifest, fetching their
 * CSS and keeping the theme registry in line with the installed plugins (like the extension
 * host does for extensions, see lib/extensions/host.ts).
 *
 * An entry is what the theme editor exports (crates/questkeep-core/src/user_themes.rs):
 * `{ id, name, description?, css: "<file in the plugin folder>", params?, preview? }`.
 */
import { createSignal } from "solid-js";
import type { PluginInfo } from "../bindings/PluginInfo";
import { translator } from "../i18n";
import { settingsPlugins } from "../i18n/settingsPlugins";
import * as app from "./api";
import { parseHex, toHex } from "./color";
import { errorMessage } from "./errors";
import { themeEditor } from "./themeEditor";
import { getTheme, registerTheme, unregisterTheme, type ThemeDef, type ThemeParam } from "./themes";
import { refreshUserThemes } from "./userThemeStore";
import { isUserThemeId, previewSwatches } from "./userThemes";

const tr = translator(settingsPlugins);

/** The largest theme CSS file that is loaded. */
export const MAX_THEME_CSS_BYTES = 1024 * 1024;
const MAX_TEXT_LENGTH = 400;
const MAX_PARAMS = 64;
const MAX_OPTIONS = 64;
const FETCH_TIMEOUT_MS = 15_000;

const THEME_ID = /^[a-z0-9][a-z0-9-]{1,63}$/;
const PARAM_KEY = /^[a-z0-9][a-z0-9-]{0,47}$/i;
/** What a select's value may be: it becomes an attribute value and a CSS value. */
const OPTION_VALUE = /^[a-z0-9][a-z0-9_.-]{0,47}$/i;
const UNIT = /^(%|[a-z]{1,8})$/i;

/** A theme entry of a plugin manifest, checked and normalized. */
export interface PluginThemeEntry {
  id: string;
  name: string;
  description?: string;
  /** The CSS file, relative to the plugin folder (forward slashes, no `..`). */
  css: string;
  params: ThemeParam[];
  /** The manifest's swatches when it has valid ones. */
  preview?: { light: string[]; dark: string[] };
}

/** Why a theme of a plugin is not available. */
export interface PluginThemeProblem {
  /** The theme's name (or id, or its position in the manifest when it has neither). */
  name: string;
  error: string;
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const text = (value: unknown): string | undefined =>
  typeof value === "string" && value.trim() !== "" && value.length <= MAX_TEXT_LENGTH
    ? value.trim()
    : undefined;

const finite = (value: unknown): value is number =>
  typeof value === "number" && Number.isFinite(value);

/** A color as `#rrggbb` (theme parameters are opaque colors); undefined when it is not a hex color. */
function opaqueHex(value: unknown): string | undefined {
  const color = typeof value === "string" ? parseHex(value) : null;
  return color ? toHex({ ...color, a: 1 }) : undefined;
}

/** One parameter of a manifest as a `ThemeParam`; null when it is malformed. */
export function parseThemeParam(raw: unknown): ThemeParam | null {
  if (!isRecord(raw)) return null;
  const key = typeof raw.key === "string" && PARAM_KEY.test(raw.key) ? raw.key : undefined;
  const label = text(raw.label);
  if (key === undefined || label === undefined) return null;
  const hint = text(raw.hint);
  const base = {
    key,
    label,
    ...(raw.advanced === true ? { advanced: true } : {}),
    ...(hint === undefined ? {} : { hint }),
  };
  switch (raw.type) {
    case "color": {
      if (isRecord(raw.default)) {
        const light = opaqueHex(raw.default.light);
        const dark = opaqueHex(raw.default.dark);
        return light && dark ? { ...base, type: "color", default: { light, dark } } : null;
      }
      const color = opaqueHex(raw.default);
      return color ? { ...base, type: "color", default: color } : null;
    }
    case "number": {
      const { min, max, step, unit } = raw;
      if (!finite(raw.default) || !finite(min) || !finite(max) || min > max) return null;
      return {
        ...base,
        type: "number",
        default: Math.min(max, Math.max(min, raw.default)),
        min,
        max,
        ...(finite(step) && step > 0 ? { step } : {}),
        ...(typeof unit === "string" && UNIT.test(unit) ? { unit } : {}),
      };
    }
    case "select": {
      if (!Array.isArray(raw.options) || raw.options.length > MAX_OPTIONS) return null;
      const options: { value: string; label: string }[] = [];
      for (const option of raw.options as unknown[]) {
        if (!isRecord(option)) return null;
        const value = option.value;
        if (typeof value !== "string" || !OPTION_VALUE.test(value)) return null;
        if (options.some((o) => o.value === value)) return null;
        options.push({ value, label: text(option.label) ?? value });
      }
      const fallback = raw.default;
      if (typeof fallback !== "string" || !options.some((o) => o.value === fallback)) return null;
      return { ...base, type: "select", default: fallback, options };
    }
    case "toggle":
      return typeof raw.default === "boolean"
        ? { ...base, type: "toggle", default: raw.default }
        : null;
    default:
      return null;
  }
}

/** The well-formed parameters of a manifest entry, each key once; the rest is dropped. */
export function parseThemeParams(raw: unknown): { params: ThemeParam[]; dropped: number } {
  if (raw === undefined || raw === null) return { params: [], dropped: 0 };
  if (!Array.isArray(raw)) return { params: [], dropped: 1 };
  const params: ThemeParam[] = [];
  let dropped = 0;
  for (const item of raw as unknown[]) {
    const param = params.length < MAX_PARAMS ? parseThemeParam(item) : null;
    if (param && !params.some((p) => p.key.toLowerCase() === param.key.toLowerCase())) {
      params.push(param);
    } else dropped++;
  }
  return { params, dropped };
}

/** Four colors as hex; undefined for anything else. */
function parseSwatches(value: unknown): string[] | undefined {
  if (!Array.isArray(value) || value.length !== 4) return undefined;
  const colors = (value as unknown[]).map((item) => {
    const color = typeof item === "string" ? parseHex(item) : null;
    return color ? toHex(color) : undefined;
  });
  return colors.every((c): c is string => c !== undefined) ? colors : undefined;
}

/** Four colors per scheme; undefined when the manifest has none or broken ones. */
function parsePreview(raw: unknown): PluginThemeEntry["preview"] {
  if (!isRecord(raw)) return undefined;
  const light = parseSwatches(raw.light);
  const dark = parseSwatches(raw.dark);
  return light && dark ? { light, dark } : undefined;
}

/** A file path inside the plugin folder: relative, forward slashes, no `.` or `..` parts. */
function isPluginPath(path: string): boolean {
  return (
    path.length <= 256 &&
    !/[\\:?#*"<>|]/.test(path) &&
    ![...path].some((c) => c.charCodeAt(0) < 0x20) &&
    path.split("/").every((part) => part !== "" && part !== "." && part !== "..")
  );
}

export type ParsedThemeEntry =
  | { ok: true; entry: PluginThemeEntry; droppedParams: number }
  | { ok: false; problem: PluginThemeProblem };

/**
 * Checks one `themes` entry of a plugin's manifest. `position` (from 0) names an entry that has
 * neither a name nor an id.
 */
export function parseThemeEntry(pluginId: string, raw: unknown, position = 0): ParsedThemeEntry {
  const record = isRecord(raw) ? raw : {};
  const id = typeof record.id === "string" ? record.id : "";
  const name = text(record.name);
  const fail = (error: string): ParsedThemeEntry => ({
    ok: false,
    problem: { name: name ?? (id || `#${position + 1}`), error },
  });
  if (!isRecord(raw)) return fail(tr("themeNotObject"));
  if (!THEME_ID.test(id)) return fail(tr("themeBadId"));
  if (id !== pluginId && !id.startsWith(`${pluginId}-`)) {
    return fail(tr("themeForeignId", { id, plugin: pluginId }));
  }
  if (isUserThemeId(id)) return fail(tr("themeTakenId", { id }));
  if (name === undefined) return fail(tr("themeNoName"));
  const css = typeof record.css === "string" ? record.css.replace(/^(\.\/)+/, "") : "";
  if (!isPluginPath(css)) return fail(tr("themeBadCss"));
  const description = text(record.description);
  const preview = parsePreview(record.preview);
  const { params, dropped } = parseThemeParams(record.params);
  return {
    ok: true,
    droppedParams: dropped,
    entry: {
      id,
      name,
      css,
      params,
      ...(description === undefined ? {} : { description }),
      ...(preview === undefined ? {} : { preview }),
    },
  };
}

// ---------------------------------------------------------------------------------------------
// CSS

/** `a/b/../c` → `a/c`; null when the path leaves the plugin folder. */
function normalizePath(path: string): string | null {
  const parts: string[] = [];
  for (const part of path.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (parts.pop() === undefined) return null;
    } else parts.push(part);
  }
  return parts.length === 0 ? null : parts.join("/");
}

const safeDecode = (value: string) => {
  try {
    return decodeURI(value);
  } catch {
    return value;
  }
};

/** `url(...)` with an unquoted, single- or double-quoted reference. */
const CSS_URL = /url\(\s*(?:"([^"\n]*)"|'([^'\n]*)'|([^'")\s][^)\s]*))\s*\)/gi;
/** References that are left as they are: any scheme, protocol-relative, absolute, a fragment. */
const NOT_RELATIVE = /^([a-z][a-z0-9+.-]*:|\/|#)/i;

/** Whether the `url()` at `offset` belongs to an `@import` (those are not supported). */
function inImport(css: string, offset: number): boolean {
  const start = Math.max(
    css.lastIndexOf(";", offset - 1),
    css.lastIndexOf("{", offset - 1),
    css.lastIndexOf("}", offset - 1),
  );
  return /^\s*(\/\*[\s\S]*?\*\/\s*)*@import\b/i.test(css.slice(start + 1, offset));
}

/**
 * Theme CSS with its relative `url(...)` references pointing at the plugin's files, so a theme can
 * ship fonts and images. `cssPath` is the CSS file (references are relative to its folder) and
 * `fileUrl` gives the URL of a file in the plugin folder. `data:`, `http(s):` and other absolute
 * references, fragments, paths that leave the plugin folder and `@import` stay untouched.
 */
export function rewriteCssUrls(
  css: string,
  cssPath: string,
  fileUrl: (path: string) => string,
): string {
  const folder = cssPath.includes("/") ? cssPath.slice(0, cssPath.lastIndexOf("/") + 1) : "";
  return css.replace(
    CSS_URL,
    (match, double?: string, single?: string, bare?: string, offset?: number) => {
      const reference = (double ?? single ?? bare ?? "").trim();
      if (reference === "" || NOT_RELATIVE.test(reference)) return match;
      if (inImport(css, offset ?? 0)) return match;
      const cut = reference.search(/[?#]/);
      const file = cut < 0 ? reference : reference.slice(0, cut);
      const suffix = cut < 0 ? "" : reference.slice(cut);
      const path = normalizePath(folder + safeDecode(file));
      if (path === null) return match;
      return `url("${fileUrl(path)}${suffix.replace(/["\\\n]/g, "")}")`;
    },
  );
}

/** What a theme definition needs from its plugin. */
export type ThemePlugin = Pick<PluginInfo, "id" | "name" | "author">;

/** A checked entry and its CSS as a registrable theme. */
export function pluginThemeDef(
  plugin: ThemePlugin,
  entry: PluginThemeEntry,
  css: string,
  fileUrl: (path: string) => string,
): ThemeDef {
  return {
    id: entry.id,
    name: entry.name,
    ...(entry.description === undefined ? {} : { description: entry.description }),
    ...(plugin.author ? { author: plugin.author } : {}),
    css: rewriteCssUrls(css, entry.css, fileUrl),
    params: entry.params,
    origin: "plugin",
    plugin: { id: plugin.id, name: plugin.name },
    // Swatches computed from the CSS itself where the manifest has none (or broken ones).
    preview: entry.preview ?? previewSwatches(css, entry.id, entry.params),
  };
}

// ---------------------------------------------------------------------------------------------
// Following the plugin list

/** Reads a plugin file as text. */
export type Fetcher = (url: string) => Promise<string>;

const fetchText: Fetcher = async (url) => {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.text();
};
let fetcher = fetchText;
/** Replaces how theme files are read (tests); null brings back `fetch`. */
export function setThemeFetcher(replacement: Fetcher | null) {
  fetcher = replacement ?? fetchText;
}

function within<T>(promise: Promise<T>, ms: number, message: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(message)), ms);
    promise.then(resolve, reject).finally(() => clearTimeout(timer));
  });
}

const byteLength = (value: string) => new TextEncoder().encode(value).length;

interface Loaded {
  /** `<plugin id>@<revision>`: another key means the plugin was loaded again. */
  key: string;
  /** The ids of the themes this plugin has in the registry. */
  ids: string[];
}

/** What is registered per plugin id (also with no themes, so a failure is not retried). */
const loaded = new Map<string, Loaded>();
/** The plugins whose themes should be registered, as of the last `syncPluginThemes`. */
const wanted = new Map<string, PluginInfo>();
/** The pending work per plugin id: loads of one plugin run one after the other. */
const queues = new Map<string, Promise<void>>();
/** Counts resets, so work that was under way before one does not register anything after it. */
let generation = 0;

const [pluginThemeProblems, setProblems] = createSignal<
  Readonly<Record<string, readonly PluginThemeProblem[]>>
>({});
/** The themes that could not be loaded, by plugin id (plugins without problems are missing). */
export { pluginThemeProblems };

function setPluginProblems(id: string, problems: PluginThemeProblem[]) {
  setProblems(({ [id]: _old, ...rest }) =>
    problems.length === 0 ? rest : { ...rest, [id]: problems },
  );
}

const keyOf = (plugin: PluginInfo) => `${plugin.id}@${plugin.revision}`;
const themeDefsOf = (plugin: PluginInfo): unknown[] =>
  Array.isArray(plugin.themeDefs) ? plugin.themeDefs : [];

/**
 * A plugin file's URL with the plugin's load counter as a path segment (`@<revision>/`, which the
 * shell skips): the webview caches by URL, and files the CSS refers to are fresh that way too.
 */
const fileUrlOf = (plugin: PluginInfo) => (path: string) =>
  app.pluginFileUrl(plugin.id, `@${plugin.revision}/${path}`);

/** Whether another plugin, a built-in theme or a user theme has this theme id. */
function isTaken(pluginId: string, themeId: string): boolean {
  for (const [owner, entry] of loaded) {
    if (owner !== pluginId && entry.ids.includes(themeId)) return true;
  }
  const existing = getTheme(themeId);
  return existing !== undefined && existing.origin !== "plugin";
}

/** The plugin's themes, read and checked; what fails goes to `problems`. */
async function build(plugin: PluginInfo, problems: PluginThemeProblem[]): Promise<ThemeDef[]> {
  const fileUrl = fileUrlOf(plugin);
  /** Per manifest entry, in its order: the theme, or why there is none. */
  const results: Promise<ThemeDef | PluginThemeProblem>[] = [];
  const seen = new Set<string>();
  for (const [position, raw] of themeDefsOf(plugin).entries()) {
    const parsed = parseThemeEntry(plugin.id, raw, position);
    if (!parsed.ok) {
      results.push(Promise.resolve(parsed.problem));
      continue;
    }
    const { entry } = parsed;
    const fail = (error: string): PluginThemeProblem => ({ name: entry.name, error });
    if (seen.has(entry.id) || isTaken(plugin.id, entry.id)) {
      results.push(Promise.resolve(fail(tr("themeTakenId", { id: entry.id }))));
      continue;
    }
    seen.add(entry.id);
    if (parsed.droppedParams > 0) {
      console.warn(
        `[plugin ${plugin.id}] theme "${entry.id}": ${parsed.droppedParams} malformed parameter(s) were left out`,
      );
    }
    results.push(
      within(
        fetcher(`${fileUrl(entry.css)}?v=${plugin.revision}`),
        FETCH_TIMEOUT_MS,
        tr("themeTimeout"),
      ).then(
        (css) => {
          if (typeof css !== "string") return fail(`${entry.css}: ${tr("themeNotText")}`);
          if (byteLength(css) > MAX_THEME_CSS_BYTES) {
            return fail(`${entry.css}: ${tr("themeTooLarge")}`);
          }
          return pluginThemeDef(plugin, entry, css, fileUrl);
        },
        (error: unknown) => fail(`${entry.css}: ${errorMessage(error)}`),
      ),
    );
  }
  const defs: ThemeDef[] = [];
  for (const result of await Promise.all(results)) {
    if ("css" in result) defs.push(result);
    else problems.push(result);
  }
  return defs;
}

/** User themes copy their base's CSS when registered: register them (and an open draft) again. */
function refreshDerived() {
  const editing = themeEditor.isOpen() ? themeEditor.draft().id : undefined;
  refreshUserThemes(editing);
  themeEditor.refresh();
}

/** Brings one plugin's themes to what is wanted now (not what was when this was queued). */
async function reconcile(id: string, started: number): Promise<void> {
  const target = wanted.get(id);
  const current = loaded.get(id);
  if (current?.key === (target && keyOf(target))) return;
  const problems: PluginThemeProblem[] = [];
  const built = target ? await build(target, problems) : [];
  if (started !== generation) return;
  // Checked again now that the files are read: another plugin may have registered the id since.
  const defs = built.filter((def) => {
    if (!isTaken(id, def.id)) return true;
    problems.push({ name: def.name, error: tr("themeTakenId", { id: def.id }) });
    return false;
  });
  // The plugin was loaded again (or went away) meanwhile: what was read is stale, and the run
  // queued by that change brings the themes up to date.
  const latest = wanted.get(id);
  if ((latest && keyOf(latest)) !== (target && keyOf(target))) return;
  // Replaced in place (never unregistered first), so a chosen theme never falls back to the
  // default one while its plugin reloads.
  const ids = defs.map((def) => def.id);
  for (const def of defs) registerTheme(def);
  for (const old of current?.ids ?? []) if (!ids.includes(old)) unregisterTheme(old);
  if (target) loaded.set(id, { key: keyOf(target), ids });
  else loaded.delete(id);
  setPluginProblems(id, problems);
  if (ids.length > 0 || (current?.ids.length ?? 0) > 0) refreshDerived();
}

/**
 * Registers, replaces and unregisters themes so they match the installed plugins: the themes of
 * every enabled plugin, read again whenever its revision changes. Resolves once every plugin's
 * themes are settled (registered, failed or gone); never rejects.
 */
export function syncPluginThemes(plugins: readonly PluginInfo[] | null | undefined): Promise<void> {
  const ids = new Set([...wanted.keys(), ...loaded.keys()]);
  wanted.clear();
  for (const plugin of plugins ?? []) {
    if (plugin.enabled && themeDefsOf(plugin).length > 0) wanted.set(plugin.id, plugin);
  }
  for (const id of wanted.keys()) ids.add(id);
  const started = generation;
  const runs: Promise<void>[] = [];
  for (const id of ids) {
    const run = (queues.get(id) ?? Promise.resolve())
      .then(() => reconcile(id, started))
      .catch((error: unknown) => console.error(`[plugin ${id}] loading its themes failed`, error));
    queues.set(id, run);
    runs.push(run);
  }
  return Promise.all(runs).then(() => undefined);
}

/** Takes every plugin theme out of the registry and forgets the plugins (tests). */
export function resetPluginThemes() {
  generation++;
  for (const entry of loaded.values()) for (const id of entry.ids) unregisterTheme(id);
  loaded.clear();
  wanted.clear();
  queues.clear();
  setProblems({});
}
