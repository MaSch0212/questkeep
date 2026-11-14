/** The kinds of settings a plugin can declare; the app draws a control for each. */
export const SETTING_TYPES = ["toggle", "text", "number", "select", "color"] as const;
export type SettingType = (typeof SETTING_TYPES)[number];
export type SettingValue = boolean | number | string;

/** A declared setting after validation: every optional part has a definite value. */
export interface DeclaredSetting {
  key: string;
  label: string;
  hint: string | null;
  type: SettingType;
  default: SettingValue;
  /** number only. */
  min: number | null;
  max: number | null;
  step: number | null;
  /** select only (never empty for a select). */
  options: { value: string; label: string }[];
}

/** The `extension` part of a plugin manifest after validation. */
export interface ExtensionManifest {
  main: string;
  styles: string[];
  settings: DeclaredSetting[];
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const finite = (value: unknown): number | null =>
  typeof value === "number" && Number.isFinite(value) ? value : null;

/** A file inside the plugin folder: relative, forward slashes, no `..`, no scheme or query. */
export function isPluginPath(value: unknown): value is string {
  if (typeof value !== "string" || value.trim() === "" || /[\\:?#]/.test(value)) return false;
  const path = value.replace(/^(\.\/)+/, "");
  return !path.startsWith("/") && path.split("/").every((part) => part !== "" && part !== "..");
}

/** `#rgb` or `#rrggbb` as lowercase `#rrggbb`; null for anything else. */
export function normalizeColor(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const text = value.trim().toLowerCase();
  if (/^#[0-9a-f]{6}$/.test(text)) return text;
  const short = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/.exec(text);
  return short ? `#${short[1]}${short[1]}${short[2]}${short[2]}${short[3]}${short[3]}` : null;
}

/**
 * A value as the setting's type, or undefined when it cannot be one (a stored value from an
 * older version of the plugin, text typed into a number field).
 */
export function coerceValue(setting: DeclaredSetting, value: unknown): SettingValue | undefined {
  switch (setting.type) {
    case "toggle":
      return typeof value === "boolean" ? value : undefined;
    case "text":
      return typeof value === "string" ? value : undefined;
    case "number": {
      const number =
        typeof value === "string" && value.trim() !== "" ? Number(value) : finite(value);
      if (number === null || !Number.isFinite(number)) return undefined;
      const atLeast = setting.min === null ? number : Math.max(setting.min, number);
      return setting.max === null ? atLeast : Math.min(setting.max, atLeast);
    }
    case "select":
      return setting.options.some((option) => option.value === value)
        ? (value as string)
        : undefined;
    case "color":
      return normalizeColor(value) ?? undefined;
  }
}

function typeDefault(setting: DeclaredSetting): SettingValue {
  switch (setting.type) {
    case "toggle":
      return false;
    case "text":
      return "";
    case "number":
      return setting.min ?? 0;
    case "select":
      return setting.options[0]?.value ?? "";
    case "color":
      return "#000000";
  }
}

function normalizeOptions(raw: unknown): { value: string; label: string }[] {
  if (!Array.isArray(raw)) return [];
  const seen = new Set<string>();
  const options: { value: string; label: string }[] = [];
  for (const entry of raw) {
    if (!isRecord(entry) || typeof entry.value !== "string" || seen.has(entry.value)) continue;
    seen.add(entry.value);
    const label = typeof entry.label === "string" && entry.label !== "" ? entry.label : entry.value;
    options.push({ value: entry.value, label });
  }
  return options;
}

/**
 * The declared settings that are usable: entries without a key, label or known type are left
 * out, as are repeated keys and selects without options. Defaults that don't fit the type fall
 * back to the type's own default.
 */
export function normalizeSettings(raw: unknown): DeclaredSetting[] {
  if (!Array.isArray(raw)) return [];
  const settings: DeclaredSetting[] = [];
  const keys = new Set<string>();
  for (const entry of raw) {
    if (!isRecord(entry)) continue;
    const { key, label, type } = entry;
    if (typeof key !== "string" || key === "" || keys.has(key)) continue;
    if (typeof label !== "string" || label === "") continue;
    if (!(SETTING_TYPES as readonly unknown[]).includes(type)) continue;
    const min = finite(entry.min);
    const max = finite(entry.max);
    const step = finite(entry.step);
    const setting: DeclaredSetting = {
      key,
      label,
      hint: typeof entry.hint === "string" && entry.hint !== "" ? entry.hint : null,
      type: type as SettingType,
      default: false,
      min: type === "number" ? min : null,
      // A maximum below the minimum is ignored.
      max: type === "number" && (min === null || max === null || max >= min) ? max : null,
      step: type === "number" && step !== null && step > 0 ? step : null,
      options: type === "select" ? normalizeOptions(entry.options) : [],
    };
    if (setting.type === "select" && setting.options.length === 0) continue;
    setting.default = coerceValue(setting, entry.default) ?? typeDefault(setting);
    keys.add(key);
    settings.push(setting);
  }
  return settings;
}

/** The manifest's `extension` object, or null when it has no usable `main`. */
export function parseExtension(raw: unknown): ExtensionManifest | null {
  if (!isRecord(raw) || !isPluginPath(raw.main)) return null;
  return {
    main: raw.main,
    styles: Array.isArray(raw.styles) ? raw.styles.filter(isPluginPath) : [],
    settings: normalizeSettings(raw.settings),
  };
}

/** The default of every declared setting by key. */
export function settingDefaults(
  settings: readonly DeclaredSetting[],
): Record<string, SettingValue> {
  return Object.fromEntries(settings.map((setting) => [setting.key, setting.default]));
}
