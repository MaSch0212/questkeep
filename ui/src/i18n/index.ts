import { createSignal } from "solid-js";

/** The UI languages, English first (the fallback). */
export const LOCALES = ["en", "de", "fr", "it", "es", "pt"] as const;
export type Locale = (typeof LOCALES)[number];

/** Each language by its own name, for the language picker. */
export const LOCALE_NAMES: Record<Locale, string> = {
  en: "English",
  de: "Deutsch",
  fr: "Français",
  it: "Italiano",
  es: "Español",
  pt: "Português",
};

/** One message: plain text with `{name}` placeholders, or a function for plurals and grammar. */
// oxlint-disable-next-line typescript/no-explicit-any -- each message declares its own params
export type Message = string | ((params: any) => string);
export type Messages = Record<string, Message>;

/**
 * The messages of one area in every language. English defines the keys (and the params of each
 * function); every other language must have exactly the same keys and compatible functions.
 */
export type Translations<M extends Messages> = { en: M } & Record<
  Exclude<Locale, "en">,
  { [K in keyof M]: M[K] extends string ? string : M[K] }
>;

/** The language of a BCP 47 tag ("de-AT" -> "de"), if the UI has it. */
export function localeOf(tag: string | null | undefined): Locale | null {
  const base = tag?.toLowerCase().split(/[-_]/)[0];
  return (LOCALES as readonly string[]).includes(base ?? "") ? (base as Locale) : null;
}

/** The chosen language, else the first supported system language, else English. */
export function resolveLocale(chosen: string | null, system: readonly string[]): Locale {
  return localeOf(chosen) ?? system.map(localeOf).find((l) => l !== null) ?? "en";
}

const systemLanguages = (): readonly string[] =>
  typeof navigator === "undefined" ? [] : (navigator.languages ?? [navigator.language]);

/** Where the last chosen language is kept, so the first paint is already in it. */
const STORAGE_KEY = "questkeep.language";

function storedChoice(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

const [locale, setLocaleSignal] = createSignal<Locale>(
  resolveLocale(storedChoice(), systemLanguages()),
);
const [choice, setChoiceSignal] = createSignal<string | null>(storedChoice());

/** The UI language now. */
export { locale };

/** The language setting: a locale, or null for the system language. */
export { choice as languageChoice };

/** Applies the language setting (null: the system language). */
export function setLanguage(chosen: string | null) {
  setChoiceSignal(chosen);
  setLocaleSignal(resolveLocale(chosen, systemLanguages()));
  try {
    if (chosen === null) localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, chosen);
  } catch {
    // Without storage the first paint uses the system language.
  }
  if (typeof document !== "undefined") document.documentElement.lang = locale();
}

/**
 * The tag for `Intl` and `toLocale…String`: the full system tag when it is the UI language (so
 * "de-AT" keeps its date format), else the language alone.
 */
export function intlLocale(): string {
  const current = locale();
  return systemLanguages().find((tag) => localeOf(tag) === current) ?? current;
}

/** Fills `{name}` placeholders. */
function fill(text: string, params?: Record<string, unknown>): string {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (all, name: string) =>
    name in params ? String(params[name]) : all,
  );
}

type Params<T> = T extends (params: infer P) => string ? P : Record<string, string | number>;

/**
 * A translator for one area: `tr("key")` or `tr("key", { count: 2 })`. Reads the current
 * language, so JSX that calls it updates when the language changes.
 */
export function translator<M extends Messages>(dict: Translations<M>) {
  return <K extends keyof M & string>(
    key: K,
    ...params: M[K] extends string ? [Record<string, string | number>?] : [Params<M[K]>]
  ): string => {
    const message: Message = (dict[locale()] as M)[key] ?? dict.en[key] ?? key;
    const [values] = params;
    return typeof message === "function"
      ? message(values)
      : fill(message, values as Record<string, unknown>);
  };
}

/** "one" or "other" style plural pick for the current language. */
export function plural(count: number, one: string, other: string): string {
  return new Intl.PluralRules(locale()).select(count) === "one" ? one : other;
}
