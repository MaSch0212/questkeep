/**
 * Questkeep extension API, version 1.
 *
 * An extension is the JavaScript part of a plugin: an ES module (`extension.main` in
 * `plugin.json`) that Questkeep imports into its window and hands an `api` object.
 *
 * ```js
 * // main.js
 * // JSDoc gives it types: @param {import("../questkeep-plugin").QuestkeepApi} api
 * export function activate(api) {
 *   api.ui.addTitleBarButton({
 *     id: "hello",
 *     label: "Say hello",
 *     icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor"><circle cx="12" cy="12" r="9"/></svg>',
 *     onClick: () => api.ui.toast("Hello from " + api.plugin.name),
 *   });
 * }
 * ```
 *
 * Everything registered through `api` (buttons, menu entries, pages, listeners, styles, open
 * dialogs) is removed automatically when the plugin is disabled, removed or reloaded. Keeping
 * the returned `Disposable` is only needed to take something back earlier.
 *
 * Extensions are trusted code: they run inside the app's window with the app's rights.
 *
 * Copy this file next to your plugin (or reference it by path) to get completion and type
 * checking in plain JavaScript through JSDoc, or import the types from TypeScript.
 */

/** Something that can be taken back. Calling `dispose()` more than once does nothing. */
export interface Disposable {
  dispose(): void;
}

/** What `activate` may return: nothing, or its own cleanup (run before the plugin unloads). */
export type ActivateResult = void | Disposable | (() => void);

/** The shape of the module named by `extension.main`. */
export interface ExtensionModule {
  /**
   * Called once after the module was imported. Register everything here. May be async; the
   * extension counts as active once the returned promise resolves. Throwing (or rejecting)
   * marks the extension as failed and removes whatever it registered so far.
   */
  activate(api: QuestkeepApi): ActivateResult | Promise<ActivateResult>;
  /** Called before the plugin is unloaded or reloaded, before its registrations are removed. */
  deactivate?(): void | Promise<void>;
}

/**
 * Draws own UI into `container` with plain DOM. The returned function (if any) is called when
 * the container goes away: remove timers and listeners there. The app's CSS classes (`btn`,
 * `input`, `check`, `muted`, `field__hint`, …) and `--qk-*` variables are the styling contract.
 */
export type Render = (container: HTMLElement) => void | (() => void);

/**
 * A game (or app) of the library. The fields below are the ones extensions usually need; every
 * other field the app knows (see `ui/src/bindings/Game.ts` in the Questkeep repository) is
 * there too and reachable by name.
 */
export interface Game {
  /** Questkeep's own id of the library entry. */
  id: string;
  name: string;
  /** The id of the store the entry comes from (`steam`, `gog`, `manual`, a plugin store, …). */
  source: string;
  /** The store's own id of the game, when it has one. */
  sourceGameId: string | null;
  installed: boolean;
  installDir: string | null;
  /** Total playtime in seconds. */
  playtimeSeconds: number;
  playCount: number;
  /** ISO 8601 date and time, or null when never played. */
  lastPlayed: string | null;
  /** ISO 8601 date and time. */
  dateAdded: string;
  favorite: boolean;
  hidden: boolean;
  kind: "game" | "app";
  description: string | null;
  releaseDate: string | null;
  developers: string[];
  publishers: string[];
  genres: string[];
  tags: string[];
  /** Ids of the game's categories. */
  categories: string[];
  /** The id of the completion status, if one is set. */
  completionStatus: string | null;
  /** The same game in several stores: one entry per store copy (at least one). */
  copies: { id: string; source: string; installed: boolean; [key: string]: unknown }[];
  [key: string]: unknown;
}

/**
 * Where menu entries can be added:
 * - `app`: the Questkeep menu in the title bar (above Settings and Exit),
 * - `add`: the "+" (Add game) menu,
 * - `game`: the context menu of one game (`context.game`),
 * - `games`: the context menu of several selected games (`context.games`),
 * - `gameMore`: the "more" menu on a game's page and details panel (`context.game`).
 */
export type MenuLocation = "app" | "add" | "game" | "games" | "gameMore";

/** What a menu was opened for. */
export interface MenuContext {
  /** The game of a `game` or `gameMore` menu. */
  game?: Game;
  /** The games a `game`, `games` or `gameMore` menu acts on (one or more). */
  games?: Game[];
}

/** One row of a menu: an action, a submenu or a separator line. */
export type MenuItem =
  | {
      label: string;
      /** SVG markup; use `currentColor` so it follows the theme. */
      icon?: string;
      /** Small text at the end of the row (a shortcut, a count). */
      hint?: string;
      /** Marks a destructive action. */
      danger?: boolean;
      disabled?: boolean;
      /** Shows a check mark column: `true` draws the mark, `false` leaves it empty. */
      checked?: boolean;
      onSelect(): void;
    }
  | { label: string; icon?: string; items: MenuItem[] }
  | { separator: true };

/** A button in the title bar, left of Settings. */
export interface TitleBarButton {
  /** Unique within the plugin. */
  id: string;
  /** The accessible name and the tooltip. */
  label: string;
  /** SVG markup; use `currentColor` so it follows the theme. */
  icon: string;
  /** `anchor` is the button itself, e.g. for `ui.showMenu(anchor, …)`. */
  onClick(anchor: HTMLElement): void;
  /**
   * A small marker on the button (a count or a short text); null, undefined or "" for none.
   * Read again after every app event and whenever `refresh()` is called on the returned handle.
   */
  badge?: () => string | number | null | undefined;
}

/** A section in the plugin's part of the settings page, below its declared settings. */
export interface SettingsSection {
  /** Unique within the plugin. */
  id: string;
  title: string;
  render: Render;
}

/** A page of its own, listed in the sidebar under "Plugins". */
export interface Page {
  /** Unique within the plugin. */
  id: string;
  title: string;
  /** SVG markup for the sidebar; use `currentColor`. */
  icon?: string;
  render: Render;
}

/** A modal dialog. Escape, the close button and a click beside it close it. */
export interface Dialog {
  title: string;
  /** Draws the body; `close` closes the dialog. The returned function is its cleanup. */
  render: (container: HTMLElement, close: () => void) => void | (() => void);
  /** Default: `medium`. */
  width?: "small" | "medium" | "large";
}

/** An event of the app's core, e.g. `{ type: "gamesChanged", ids: [...] }`. */
export interface AppEvent {
  /**
   * `gamesChanged` (`ids`), `gameRemoved` (`id`), `categoriesChanged`, `pluginsChanged`,
   * `scansQueued`, `scanStarted`, `libraryScanned` (`provider`, `summary`), `scanFailed`,
   * `gameStarted` (`id`), `gameStopped` (`id`, `sessionSeconds`), `installFinished` (`id`),
   * `gameError`, `backgroundWork`, `backupProgress`, `backupFinished`, …
   */
  type: string;
  [key: string]: unknown;
}

export interface QuestkeepApi {
  /** The version of this API. Check it before using something added later. */
  readonly apiVersion: 1;

  /** The plugin this extension belongs to (from its `plugin.json`). */
  readonly plugin: {
    readonly id: string;
    readonly name: string;
    readonly version: string;
    /**
     * The URL of a file in the plugin folder, e.g. `api.plugin.url("images/logo.png")` for an
     * `<img>`, a `fetch` or a dynamic `import()`. It changes with every reload of the plugin,
     * so the current file is loaded, not a cached one.
     */
    url(path: string): string;
  };

  /** Additions to the app's interface. */
  readonly ui: {
    /** Adds a button to the title bar. `refresh()` re-reads its badge. */
    addTitleBarButton(button: TitleBarButton): Disposable & { refresh(): void };
    /**
     * Adds entries to one of the app's menus. `items` is called every time the menu opens, so
     * it can depend on the game(s) the menu is for; return `[]` to add nothing.
     */
    addMenuItems(location: MenuLocation, items: (context: MenuContext) => MenuItem[]): Disposable;
    /** Adds a section with own UI to the plugin's settings (Settings, under the plugin's name). */
    addSettingsSection(section: SettingsSection): Disposable;
    /** Adds a page to the sidebar; `open()` shows it. */
    addPage(page: Page): Disposable & { open(): void };
    /** Opens a modal dialog. */
    showDialog(dialog: Dialog): { close(): void };
    /** Opens a popup menu under an element or at a point of the window. */
    showMenu(anchor: HTMLElement | { x: number; y: number }, items: MenuItem[]): void;
    /** Shows a short message in the corner of the window. */
    toast(message: string, kind?: "info" | "error"): void;
    /** Adds CSS text to the page (files listed in `extension.styles` are loaded without this). */
    addStyles(css: string): Disposable;
  };

  /**
   * The plugin's own settings, saved by the app. Settings declared in `plugin.json`
   * (`extension.settings`) get a control in the settings page and a default value; any other
   * key can be stored too (JSON values).
   */
  readonly settings: {
    /** The stored value, else the declared default, else `fallback`. */
    get<T = unknown>(key: string, fallback?: T): T;
    /** Every declared default overlaid with every stored value. */
    all(): Record<string, unknown>;
    /** Stores a value (`undefined` removes it, bringing back the default). */
    set(key: string, value: unknown): Promise<void>;
    /** Called after a value changed, by `set` or by the user in the settings page. */
    onChange(listener: (key: string, value: unknown) => void): Disposable;
  };

  /** The library. */
  readonly games: {
    /** Every entry of the library: games and apps, hidden ones included, sorted by name. */
    list(): Promise<Game[]>;
    get(id: string): Promise<Game | undefined>;
    /** The games selected in the open library view (empty when none is). */
    selected(): Game[];
    onSelectionChange(listener: (games: Game[]) => void): Disposable;
    /** Shows the game's page. */
    open(id: string): void;
  };

  /** Events of the app's core. */
  readonly events: {
    /** Listens for one event type, or for all of them with `"*"`. */
    on(type: string, listener: (event: AppEvent) => void): Disposable;
  };

  /** The look of the app, for UI that cannot follow the `--qk-*` variables alone (a canvas). */
  readonly theme: {
    scheme(): "light" | "dark";
    /** Called when the scheme or the theme changes. */
    onChange(listener: () => void): Disposable;
  };

  /**
   * Calls any command of the app (`list_games`, `update_game`, …; see the command table in
   * `docs/ARCHITECTURE.md`). Rejects with a readable message.
   */
  invoke<T = unknown>(command: string, args?: Record<string, unknown>): Promise<T>;

  /** Writes to the window's console, prefixed with the plugin id. */
  readonly log: {
    debug(...args: unknown[]): void;
    info(...args: unknown[]): void;
    warn(...args: unknown[]): void;
    error(...args: unknown[]): void;
  };
}

/** A setting declared in `plugin.json` under `extension.settings`. */
export interface DeclaredSetting {
  /** The key used with `api.settings.get`. */
  key: string;
  label: string;
  hint?: string;
  type: "toggle" | "text" | "number" | "select" | "color";
  /** toggle: boolean; number: number; text, select: string; color: `#rrggbb`. */
  default?: boolean | number | string;
  /** number only. */
  min?: number;
  max?: number;
  step?: number;
  /** select only. */
  options?: { value: string; label: string }[];
}

/** The `extension` object of `plugin.json`. */
export interface ExtensionManifest {
  /** The ES module to import, relative to the plugin folder. */
  main: string;
  /** Style sheets to load with the extension, relative to the plugin folder. */
  styles?: string[];
  settings?: DeclaredSetting[];
}
