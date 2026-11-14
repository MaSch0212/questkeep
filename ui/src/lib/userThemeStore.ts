/**
 * The user's own themes (made in the theme editor) as loaded from the core, and their place in
 * the theme registry. The editor (lib/themeEditor.ts) builds on this.
 */
import { createSignal } from "solid-js";
import type { UserTheme } from "../bindings/UserTheme";
import * as api from "./api";
import {
  DEFAULT_THEME_ID,
  getTheme,
  registerTheme,
  unregisterTheme,
  type ThemeDef,
} from "./themes";
import { userThemeDef } from "./userThemes";

const [userThemes, setUserThemes] = createSignal<UserTheme[]>([]);

/** The saved user themes, by name. */
export { userThemes };

export const getUserTheme = (id: string) => userThemes().find((theme) => theme.id === id);

/** Whether the theme a user theme derives from is not registered (a plugin that is not loaded). */
export const isBaseMissing = (theme: UserTheme) =>
  theme.base === theme.id || getTheme(theme.base) === undefined;

/** The theme a user theme builds on: its base, or the default theme while that one is missing. */
export function baseTheme(theme: UserTheme): ThemeDef | undefined {
  return isBaseMissing(theme) ? getTheme(DEFAULT_THEME_ID) : getTheme(theme.base);
}

/** Puts a user theme (saved or a draft) into the theme registry under its id. */
export function registerUserTheme(theme: UserTheme): void {
  registerTheme(userThemeDef(theme, baseTheme(theme)));
}

const byName = (a: UserTheme, b: UserTheme) =>
  a.name.localeCompare(b.name, undefined, { sensitivity: "base" }) || a.id.localeCompare(b.id);

/**
 * Makes `list` the saved user themes: registers each and unregisters those that are gone.
 * `keep` is an id whose registration is left alone (the draft the editor shows under it).
 */
export function setSavedUserThemes(list: UserTheme[], keep?: string): void {
  const ids = new Set(list.map((theme) => theme.id));
  for (const old of userThemes()) {
    if (!ids.has(old.id) && old.id !== keep) unregisterTheme(old.id);
  }
  setUserThemes(list.toSorted(byName));
  for (const theme of list) if (theme.id !== keep) registerUserTheme(theme);
}

/** Loads the user themes from the core and registers them (at start, and to refresh). */
export async function loadUserThemes(keep?: string): Promise<void> {
  const list: unknown = await api.listUserThemes();
  if (Array.isArray(list)) setSavedUserThemes(list as UserTheme[], keep);
}

/**
 * Registers the saved user themes again. Call it after a theme they may derive from was
 * registered or removed (a plugin loaded): user themes copy their base's CSS when registered.
 */
export function refreshUserThemes(keep?: string): void {
  setSavedUserThemes(userThemes(), keep);
}

/** The saved list with one theme added or replaced. */
export function withSavedTheme(theme: UserTheme): UserTheme[] {
  return [...userThemes().filter((t) => t.id !== theme.id), theme];
}
