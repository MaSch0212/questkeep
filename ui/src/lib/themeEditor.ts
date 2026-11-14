/**
 * The theme editor's state: which user theme is being edited, its draft, the live preview, and
 * saving, deleting and exporting user themes. The panel (components/ThemeEditor.tsx) and the
 * theme cards in Settings → Appearance are views of this.
 *
 * While the editor is open the draft is registered under the theme's id and shown through
 * `previewAppearance`, so the saved settings stay untouched; closing shows them again.
 */
import { createSignal } from "solid-js";
import type { Appearance } from "../bindings/Appearance";
import type { Settings } from "../bindings/Settings";
import type { UserTheme } from "../bindings/UserTheme";
import { translator } from "../i18n";
import { themeEditorText } from "../i18n/themeEditor";
import * as api from "./api";
import { errorMessage } from "./errors";
import { pickDirectory } from "./pickers";
import { store } from "./store";
import {
  appliedAppearance,
  DEFAULT_THEME_ID,
  effectiveScheme,
  getTheme,
  previewAppearance,
  themes,
  unregisterTheme,
  type ParamValue,
  type Scheme,
} from "./themes";
import {
  baseTheme,
  getUserTheme,
  registerUserTheme,
  setSavedUserThemes,
  userThemes,
  withSavedTheme,
} from "./userThemeStore";
import {
  emptyOverrides,
  exportedTheme,
  isPerScheme,
  uniqueName,
  userThemeId,
  withOverride,
  withoutOverride,
  type ThemeVariable,
} from "./userThemes";

const tr = translator(themeEditorText);

/** While typing, the page follows after this pause; pickers and steppers apply at once. */
export const PREVIEW_DELAY_MS = 60;

/** Where edits go: by kind (colors and shadows to the edited mode, the rest to both), or fixed. */
export type EditTarget = "auto" | "scheme" | "both";

interface Session {
  /** The id the theme is saved under; null for a theme that was never saved. */
  savedId: string | null;
  /** What Discard goes back to, and what "unsaved changes" compares with. */
  baseline: UserTheme;
  /** Parameter values a new theme starts with (those of the theme it was made from). */
  params: Record<string, ParamValue> | undefined;
}

const NO_THEME: UserTheme = { id: "", name: "", base: "", overrides: emptyOverrides(), css: "" };

const [session, setSession] = createSignal<Session | null>(null);
const [draft, setDraft] = createSignal<UserTheme>(NO_THEME);
const [scheme, setScheme] = createSignal<Scheme>("dark");
const [target, setTarget] = createSignal<EditTarget>("auto");
/** Counts up when this module saved the settings, so a settings page can load them again. */
const [settingsSaved, setSettingsSaved] = createSignal(0);
let timer: ReturnType<typeof setTimeout> | undefined;

const same = (a: UserTheme, b: UserTheme) => JSON.stringify(a) === JSON.stringify(b);

/** Whether the editor is open. */
const isOpen = () => session() !== null;
/** Whether the draft differs from what is saved (or, for a new theme, from how it started). */
const isDirty = () => {
  const current = session();
  return current !== null && !same(draft(), current.baseline);
};
/** Whether the theme in the editor was never saved. */
const isNew = () => session()?.savedId === null;

/** Registers the draft now (cancelling a pending delayed update). */
function flush(): void {
  clearTimeout(timer);
  timer = undefined;
  if (isOpen()) registerUserTheme(draft());
}

/** Shows the draft's theme in the edited mode, whatever the settings say. */
function showPreview(): void {
  const current = session();
  if (!current) return;
  const id = draft().id;
  const mode = scheme();
  const extra = current.params;
  previewAppearance((applied: Appearance) => ({
    theme: id,
    scheme: mode,
    params: extra && !applied.params[id] ? { ...applied.params, [id]: extra } : applied.params,
  }));
}

/** Puts back what was registered under an id before the draft: the saved theme, or nothing. */
function restoreRegistration(id: string): void {
  const saved = getUserTheme(id);
  if (saved) registerUserTheme(saved);
  else unregisterTheme(id);
}

function begin(next: Session, theme: UserTheme): void {
  setSession(next);
  setDraft(theme);
  setScheme(effectiveScheme());
  setTarget("auto");
  registerUserTheme(theme);
  showPreview();
}

/** False (with a toast) when the editor holds unsaved changes that opening would lose. */
function free(): boolean {
  if (!isDirty()) {
    if (isOpen()) close();
    return true;
  }
  store.pushToast("info", tr("busy"));
  return false;
}

const takenIds = () => themes().map((theme) => theme.id);
const takenNames = () => themes().map((theme) => theme.name);

/**
 * Opens the editor on a new theme made from an existing one: a copy of a user theme, or a theme
 * based on a built-in or plugin theme (its saved parameter values come along).
 */
function create(fromId: string): boolean {
  if (!free()) return false;
  const source = getUserTheme(fromId);
  const from = getTheme(fromId) ?? getTheme(DEFAULT_THEME_ID);
  const name = uniqueName(
    tr(source ? "copyName" : "customName", { name: source?.name ?? from?.name ?? "Questkeep" }),
    takenNames(),
  );
  const theme: UserTheme = source
    ? { ...source, name, id: userThemeId(name, takenIds()) }
    : {
        id: userThemeId(name, takenIds()),
        name,
        base: from?.id ?? DEFAULT_THEME_ID,
        overrides: emptyOverrides(),
        css: "",
      };
  begin({ savedId: null, baseline: theme, params: appliedAppearance()?.params[fromId] }, theme);
  return true;
}

/** Opens the editor on a saved user theme. */
function edit(id: string): boolean {
  const theme = getUserTheme(id);
  if (!theme) return false;
  if (session()?.savedId === id) return true;
  if (!free()) return false;
  begin({ savedId: id, baseline: theme, params: undefined }, theme);
  return true;
}

/** Changes the draft. The page follows at once (`now`) or after a short pause (typing). */
function update(patch: Partial<UserTheme>, now = false): void {
  if (!isOpen()) return;
  setDraft({ ...draft(), ...patch });
  clearTimeout(timer);
  if (now) flush();
  else timer = setTimeout(flush, PREVIEW_DELAY_MS);
}

/** The scheme (or both) a change to a variable goes to with the current "applies to" choice. */
function targetOf(variable: ThemeVariable): Scheme | "both" {
  const choice = target();
  if (choice === "both" || (choice === "auto" && !isPerScheme(variable))) return "both";
  return scheme();
}

/** Sets a variable's override in the draft; `undefined` removes it (back to the base theme). */
function setOverride(variable: ThemeVariable, value: string | undefined, now = false): void {
  const overrides = draft().overrides;
  update(
    {
      overrides:
        value === undefined
          ? withoutOverride(overrides, variable.name, scheme())
          : withOverride(overrides, variable.name, value, targetOf(variable), scheme()),
    },
    now,
  );
}

/** Switches which mode is edited; the page shows that mode while the editor is open. */
function setEditScheme(next: Scheme): void {
  setScheme(next);
  showPreview();
}

/** Throws the changes away: back to the saved theme (or to how the new theme started). */
function discard(): void {
  const current = session();
  if (!current) return;
  setDraft(current.baseline);
  flush();
}

/** Closes the editor without saving and shows the saved appearance again. */
function close(): void {
  const current = session();
  if (!current) return;
  clearTimeout(timer);
  timer = undefined;
  const id = draft().id;
  setSession(null);
  setDraft(NO_THEME);
  restoreRegistration(id);
  previewAppearance(null);
}

/** Saves the settings with another appearance and tells the app and open settings pages. */
async function saveAppearance(change: (appearance: Appearance) => Appearance): Promise<void> {
  const settings = await api.getSettings();
  const next: Settings = { ...settings, appearance: change(settings.appearance) };
  await api.saveSettings(next);
  store.applySettings(next);
  setSettingsSaved((n) => n + 1);
}

/**
 * Saves the draft. A new theme (also "save as copy") gets its id from its name and becomes the
 * selected theme; the editor then continues on the saved theme. Resolves to whether it worked.
 */
async function save(asCopy = false): Promise<boolean> {
  const current = session();
  if (!current) return false;
  flush();
  const draftId = draft().id;
  let name = draft().name.trim();
  if (name === "") {
    store.pushToast("error", tr("needsName"));
    return false;
  }
  const creating = asCopy || current.savedId === null;
  if (asCopy && name === current.baseline.name) {
    name = uniqueName(tr("copyName", { name }), takenNames());
  }
  const theme: UserTheme = {
    ...draft(),
    name,
    id: creating
      ? userThemeId(
          name,
          takenIds().filter((id) => id !== draftId || asCopy),
        )
      : draftId,
  };
  try {
    await api.saveUserTheme(theme);
  } catch (error) {
    store.pushToast("error", errorMessage(error));
    return false;
  }
  setSavedUserThemes(withSavedTheme(theme));
  if (theme.id !== draftId) restoreRegistration(draftId);
  setSession({ savedId: theme.id, baseline: theme, params: current.params });
  setDraft(theme);
  showPreview();
  if (creating) {
    const params = current.params ?? appliedAppearance()?.params[draftId];
    try {
      await saveAppearance((appearance) => ({
        ...appearance,
        theme: theme.id,
        params:
          params && !appearance.params[theme.id]
            ? { ...appearance.params, [theme.id]: params }
            : appearance.params,
      }));
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    }
  }
  store.pushToast("info", tr("saved", { name }));
  return true;
}

/** Deletes a saved user theme; when it was the selected theme, its base theme takes over. */
async function remove(id: string): Promise<boolean> {
  const theme = getUserTheme(id);
  if (!theme) return false;
  if (session()?.savedId === id) close();
  try {
    await api.deleteUserTheme(id);
    setSavedUserThemes(userThemes().filter((t) => t.id !== id));
    if (appliedAppearance()?.theme === id) {
      const fallback = getTheme(theme.base) ? theme.base : DEFAULT_THEME_ID;
      await saveAppearance((appearance) => {
        const { [id]: _removed, ...params } = appearance.params;
        return { ...appearance, theme: fallback, params };
      });
    }
  } catch (error) {
    store.pushToast("error", errorMessage(error));
    return false;
  }
  store.pushToast("info", tr("deleted", { name: theme.name }));
  return true;
}

/** Asks for a folder and writes a saved user theme there as a plugin. */
async function exportPlugin(id: string): Promise<string | null> {
  const theme = getUserTheme(id);
  if (!theme) return null;
  try {
    const folder = await pickDirectory();
    if (folder === null) return null;
    const path = await api.exportUserTheme(id, folder, exportedTheme(theme, baseTheme(theme)));
    store.pushToast("info", tr("exported", { path }));
    return path;
  } catch (error) {
    store.pushToast("error", errorMessage(error));
    return null;
  }
}

export const themeEditor = {
  isOpen,
  isDirty,
  isNew,
  /** The theme as edited (meaningful while the editor is open). */
  draft,
  /** The mode whose overrides are edited and previewed. */
  scheme,
  setEditScheme,
  target,
  setTarget,
  targetOf,
  create,
  edit,
  update,
  setOverride,
  discard,
  save,
  close,
  remove,
  exportPlugin,
  settingsSaved,
  /** The parameter values a theme that was never saved starts with (and is previewed with). */
  startParams: () => session()?.params,
  /** Registers the draft again (its base theme changed: a plugin was loaded again). */
  refresh: () => {
    if (isOpen()) registerUserTheme(draft());
  },
};
