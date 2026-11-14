import { createSignal, type Accessor } from "solid-js";
import type { SettingsController } from "./settings";
import type {
  AppEvent,
  Dialog,
  Disposable,
  MenuContext,
  MenuItem,
  MenuLocation,
  Page,
  SettingsSection,
  TitleBarButton,
} from "./types";

/**
 * What extensions added to the app, by plugin. Components read the lists (signals); every
 * addition is tracked, so `disposePlugin` takes back all of a plugin's additions at once.
 * This module knows nothing about the app store, so the store can report events to it.
 */

export function logPluginError(pluginId: string, what: string, error: unknown) {
  console.error(`[plugin ${pluginId}] ${what}:`, error);
}

/** Runs plugin code; a throw is logged and gives undefined. */
export function guard<T>(pluginId: string, what: string, run: () => T): T | undefined {
  try {
    return run();
  } catch (error) {
    logPluginError(pluginId, what, error);
    return undefined;
  }
}

const tracked = new Map<string, Set<() => void>>();

/**
 * Remembers how to undo something a plugin did. The undo runs once: through the returned
 * disposable or when the plugin is disposed, whichever comes first. A throwing undo is logged.
 */
export function track(pluginId: string, undo: () => void): Disposable {
  let done = false;
  const run = () => {
    if (done) return;
    done = true;
    tracked.get(pluginId)?.delete(run);
    guard(pluginId, "cleanup failed", undo);
  };
  let undos = tracked.get(pluginId);
  if (!undos) {
    undos = new Set();
    tracked.set(pluginId, undos);
  }
  undos.add(run);
  return { dispose: run };
}

/** Takes back everything the plugin added, newest first. Safe to call again. */
export function disposePlugin(pluginId: string) {
  const undos = tracked.get(pluginId);
  if (!undos) return;
  tracked.delete(pluginId);
  for (const run of [...undos].toReversed()) run();
}

/** How many additions of a plugin are still tracked (for tests and diagnostics). */
export const trackedCount = (pluginId: string) => tracked.get(pluginId)?.size ?? 0;

interface Owned {
  plugin: string;
  /** Unique in its list; adding the same key again replaces the older entry. */
  key: string;
}

function createList<T extends Owned>() {
  const [items, setItems] = createSignal<readonly T[]>([]);
  function add(entry: T): Disposable {
    setItems((list) => [...list.filter((other) => other.key !== entry.key), entry]);
    return track(entry.plugin, () => setItems((list) => list.filter((other) => other !== entry)));
  }
  return [items, add] as const;
}

let sequence = 0;
const nextKey = (pluginId: string) => `${pluginId}#${++sequence}`;

// --- Title bar buttons ---

export interface TitleBarEntry extends Owned {
  button: TitleBarButton;
  /** Changes when the badge should be read again. */
  version: Accessor<number>;
}
const [titleBarButtons, addTitleBarEntry] = createList<TitleBarEntry>();
export { titleBarButtons };

export function addTitleBarButton(
  pluginId: string,
  button: TitleBarButton,
): Disposable & { refresh(): void } {
  const [version, setVersion] = createSignal(0);
  const disposable = addTitleBarEntry({
    plugin: pluginId,
    key: `${pluginId}/${button.id}`,
    button,
    version,
  });
  return { ...disposable, refresh: () => setVersion((n) => n + 1) };
}

// --- Menu items ---

export interface MenuProvider extends Owned {
  location: MenuLocation;
  items: (context: MenuContext) => MenuItem[];
}
const [menuProviders, addMenuProvider] = createList<MenuProvider>();
export { menuProviders };

export const addMenuItems = (
  pluginId: string,
  location: MenuLocation,
  items: (context: MenuContext) => MenuItem[],
) => addMenuProvider({ plugin: pluginId, key: nextKey(pluginId), location, items });

/** Every plugin's items for a menu, plugin by plugin; a provider that throws adds nothing. */
export function menuItemsFor(
  location: MenuLocation,
  context: MenuContext,
): { plugin: string; items: MenuItem[] }[] {
  return menuProviders()
    .filter((provider) => provider.location === location)
    .flatMap((provider) => {
      const items = guard(provider.plugin, `menu items for "${location}" failed`, () =>
        provider.items(context),
      );
      return Array.isArray(items) && items.length > 0 ? [{ plugin: provider.plugin, items }] : [];
    });
}

// --- Settings sections ---

export interface SettingsSectionEntry extends Owned {
  section: SettingsSection;
}
const [settingsSections, addSettingsEntry] = createList<SettingsSectionEntry>();
export { settingsSections };

export const addSettingsSection = (pluginId: string, section: SettingsSection) =>
  addSettingsEntry({ plugin: pluginId, key: `${pluginId}/${section.id}`, section });

// --- Pages ---

/** The app page id of a plugin page: `plugin:<plugin id>/<page id>`. */
export type PluginPageId = `plugin:${string}`;
export const pluginPageId = (pluginId: string, pageId: string): PluginPageId =>
  `plugin:${pluginId}/${pageId}`;
export const isPluginPage = (page: string): page is PluginPageId => page.startsWith("plugin:");
/** The plugin a page id belongs to (plugin ids contain no slash). */
export const pluginOfPage = (page: PluginPageId) => page.slice("plugin:".length).split("/")[0];

export interface PageEntry extends Owned {
  key: PluginPageId;
  page: Page;
}
const [pages, addPageEntry] = createList<PageEntry>();
export { pages };

export const addPage = (pluginId: string, page: Page) =>
  addPageEntry({ plugin: pluginId, key: pluginPageId(pluginId, page.id), page });

export const findPage = (id: string) => pages().find((entry) => entry.key === id);

// --- Dialogs and popup menus ---

export interface DialogEntry extends Owned {
  dialog: Dialog;
  close: () => void;
}
const [dialogs, addDialogEntry] = createList<DialogEntry>();
export { dialogs };

export function openDialog(pluginId: string, dialog: Dialog): { close(): void } {
  const entry: DialogEntry = {
    plugin: pluginId,
    key: nextKey(pluginId),
    dialog,
    close: () => disposable.dispose(),
  };
  const disposable = addDialogEntry(entry);
  return { close: entry.close };
}

export interface PopupMenu {
  plugin: string;
  /** The element the menu opened under (focus returns to it), or a point of the window. */
  anchor: HTMLElement | { x: number; y: number };
  items: MenuItem[];
}
const [popupMenu, setPopupMenu] = createSignal<PopupMenu | null>(null);
export { popupMenu };

export function closePopupMenu() {
  setPopupMenu(null);
}

export function openPopupMenu(menu: PopupMenu) {
  setPopupMenu(menu);
  track(menu.plugin, () => setPopupMenu((open) => (open === menu ? null : open)));
}

// --- Events ---

interface EventListenerEntry {
  plugin: string;
  type: string;
  listener: (event: AppEvent) => void;
}
const eventListeners = new Set<EventListenerEntry>();
const [eventCount, setEventCount] = createSignal(0);
/** Counts core events; title bar badges are read again when it changes. */
export { eventCount };

export function addEventListener(
  pluginId: string,
  type: string,
  listener: (event: AppEvent) => void,
): Disposable {
  const entry = { plugin: pluginId, type, listener };
  eventListeners.add(entry);
  return track(pluginId, () => eventListeners.delete(entry));
}

/** Hands a core event to the extensions listening for it (or for `"*"`). */
export function emitCoreEvent(event: AppEvent) {
  // A copy: a listener may add or remove listeners.
  for (const entry of Array.from(eventListeners)) {
    if (entry.type !== "*" && entry.type !== event.type) continue;
    guard(entry.plugin, `"${event.type}" listener failed`, () => entry.listener(event));
  }
  setEventCount((n) => n + 1);
}

// --- Settings of the active extensions ---

const [settingsControllers, setSettingsControllers] = createSignal<
  Readonly<Record<string, SettingsController>>
>({});
/** The settings of every plugin whose extension is loaded, by plugin id. */
export { settingsControllers };

export function registerSettings(pluginId: string, controller: SettingsController): Disposable {
  setSettingsControllers((all) => ({ ...all, [pluginId]: controller }));
  return track(pluginId, () =>
    setSettingsControllers((all) => {
      if (all[pluginId] !== controller) return all;
      const { [pluginId]: _gone, ...rest } = all;
      return rest;
    }),
  );
}

// --- Reset (tests, app teardown) ---

const resetHooks = new Set<() => void>();
/** Runs when the registry is reset (the host forgets its instances). */
export function onRegistryReset(hook: () => void) {
  resetHooks.add(hook);
}

/** Takes back every plugin's additions. */
export function resetRegistry() {
  for (const hook of resetHooks) hook();
  for (const pluginId of Array.from(tracked.keys())) disposePlugin(pluginId);
  setPopupMenu(null);
}
