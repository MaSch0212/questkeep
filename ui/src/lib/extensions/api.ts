import { createEffect, createRoot, on, untrack } from "solid-js";
import * as app from "../api";
import { defaultQuery } from "../query";
import { store } from "../store";
import * as registry from "./registry";
import { guard, track } from "./registry";
import type { SettingsController } from "./settings";
import type { Disposable, Game, QuestkeepApi } from "./types";

/** The plugin an API object is made for. */
export interface ApiPlugin {
  id: string;
  name: string;
  version: string;
  /** The plugin's load counter; file URLs carry it so a reload gets fresh files. */
  revision?: number;
}

const idsOf = (games: readonly { id: string }[]) => games.map((game) => game.id).join();

/** Fails a registration with a message the plugin author can act on. */
function need(condition: boolean, message: string): asserts condition {
  if (!condition) throw new TypeError(message);
}

const isText = (value: unknown): value is string => typeof value === "string" && value !== "";
const isFunction = (value: unknown) => typeof value === "function";

/**
 * The `api` object an extension's `activate` receives. Everything it registers is tracked under
 * the plugin's id (`disposePlugin` takes it back) and every call into plugin code is guarded, so
 * a throwing listener is logged instead of breaking the app.
 */
export function createApi(plugin: ApiPlugin, settings: SettingsController): QuestkeepApi {
  const id = plugin.id;
  /** Wraps a plugin callback: a throw is logged. */
  const safe =
    <A extends unknown[]>(what: string, callback: (...args: A) => void) =>
    (...args: A) =>
      void guard(id, what, () => callback(...args));

  const log =
    (level: "debug" | "info" | "warn" | "error") =>
    (...args: unknown[]) =>
      // oxlint-disable-next-line no-console -- the extension's own log output
      console[level](`[plugin ${id}]`, ...args);

  return {
    apiVersion: 1,
    plugin: {
      id,
      name: plugin.name,
      version: plugin.version,
      url(path) {
        if (plugin.revision === undefined) return app.pluginFileUrl(id, String(path));
        const url = app.pluginFileUrl(id, `@${plugin.revision}/${path}`);
        return `${url}${url.includes("?") ? "&" : "?"}v=${plugin.revision}`;
      },
    },
    ui: {
      addTitleBarButton(button) {
        need(isText(button?.id), "addTitleBarButton: `id` must be a non-empty string");
        need(isText(button.label), "addTitleBarButton: `label` must be a non-empty string");
        need(isText(button.icon), "addTitleBarButton: `icon` must be SVG markup");
        need(isFunction(button.onClick), "addTitleBarButton: `onClick` must be a function");
        return registry.addTitleBarButton(id, button);
      },
      addMenuItems(location, items) {
        need(
          ["app", "add", "game", "games", "gameMore"].includes(location),
          `addMenuItems: unknown location "${String(location)}"`,
        );
        need(isFunction(items), "addMenuItems: `items` must be a function returning menu items");
        return registry.addMenuItems(id, location, items);
      },
      addSettingsSection(section) {
        need(isText(section?.id), "addSettingsSection: `id` must be a non-empty string");
        need(isText(section.title), "addSettingsSection: `title` must be a non-empty string");
        need(isFunction(section.render), "addSettingsSection: `render` must be a function");
        return registry.addSettingsSection(id, section);
      },
      addPage(page) {
        need(isText(page?.id), "addPage: `id` must be a non-empty string");
        need(isText(page.title), "addPage: `title` must be a non-empty string");
        need(isFunction(page.render), "addPage: `render` must be a function");
        const disposable = registry.addPage(id, page);
        return {
          ...disposable,
          open: () => store.showPluginPage(registry.pluginPageId(id, page.id)),
        };
      },
      showDialog(dialog) {
        need(isText(dialog?.title), "showDialog: `title` must be a non-empty string");
        need(isFunction(dialog.render), "showDialog: `render` must be a function");
        return registry.openDialog(id, dialog);
      },
      showMenu(anchor, items) {
        need(
          anchor instanceof HTMLElement ||
            (typeof anchor?.x === "number" && typeof anchor.y === "number"),
          "showMenu: `anchor` must be an element or a point { x, y }",
        );
        registry.openPopupMenu({ plugin: id, anchor, items });
      },
      toast(message, kind = "info") {
        store.pushToast(kind === "error" ? "error" : "info", String(message));
      },
      addStyles(css) {
        const style = document.createElement("style");
        style.dataset.plugin = id;
        style.textContent = String(css);
        document.head.append(style);
        return track(id, () => style.remove());
      },
    },
    settings: {
      get: <T>(key: string, fallback?: T) => settings.get(key, fallback) as T,
      all: () => settings.all(),
      set: (key, value) => settings.set(key, value),
      onChange(listener) {
        need(isFunction(listener), "settings.onChange: the listener must be a function");
        return track(id, settings.onChange(safe("settings listener failed", listener)));
      },
    },
    games: {
      async list() {
        // A stubbed command (tests) may answer with nothing.
        const games = await app.listGames({ ...defaultQuery(), hidden: "all", kind: "both" });
        return (games ?? []) as Game[];
      },
      async get(gameId) {
        try {
          return ((await app.getGame(gameId)) ?? undefined) as Game | undefined;
        } catch {
          return undefined; // no such game
        }
      },
      selected: () => untrack(() => [...store.selectedGames()]),
      onSelectionChange(listener) {
        need(isFunction(listener), "games.onSelectionChange: the listener must be a function");
        const call = safe("selection listener failed", listener);
        // The list reloads often; only another set of games is a change.
        let last = untrack(() => idsOf(store.selectedGames()));
        const dispose = createRoot((disposeRoot) => {
          createEffect(
            on(
              store.selectedGames,
              (games) => {
                const now = idsOf(games);
                if (now === last) return;
                last = now;
                call([...games]);
              },
              { defer: true },
            ),
          );
          return disposeRoot;
        });
        return track(id, dispose);
      },
      open(gameId) {
        const known = untrack(() => store.games().find((game) => game.id === gameId));
        if (known) {
          store.openGamePage(known);
          return;
        }
        void Promise.resolve()
          .then(() => app.getGame(gameId))
          .then((game) => game && store.openGamePage(game))
          .catch((error: unknown) => registry.logPluginError(id, "games.open failed", error));
      },
    },
    events: {
      on(type, listener) {
        need(isText(type), 'events.on: `type` must be an event type or "*"');
        need(isFunction(listener), "events.on: the listener must be a function");
        return registry.addEventListener(id, type, listener);
      },
    },
    theme: {
      scheme: () => (document.documentElement.dataset.scheme === "light" ? "light" : "dark"),
      onChange(listener): Disposable {
        need(isFunction(listener), "theme.onChange: the listener must be a function");
        const observer = new MutationObserver(safe("theme listener failed", () => listener()));
        observer.observe(document.documentElement, {
          attributes: true,
          attributeFilter: ["data-scheme", "data-theme"],
        });
        return track(id, () => observer.disconnect());
      },
    },
    invoke: (command, args) => app.invokeCommand(command, args),
    log: { debug: log("debug"), info: log("info"), warn: log("warn"), error: log("error") },
  };
}
