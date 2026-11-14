# Plugins

A plugin is a folder (or a `.zip` of it) with a `plugin.json`. It can bring:

- **stores**: game providers written in Rust, compiled to WebAssembly, run sandboxed;
- an **extension**: JavaScript that adds to the app's interface (title bar buttons, menu
  entries, settings, pages, dialogs);
- themes (planned).

Examples live in [`examples/plugins`](../examples/plugins):
[`example-store`](../examples/plugins/example-store) and
[`example-extension`](../examples/plugins/example-extension).

## The plugin package

```json
{
  "id": "my-plugin",
  "name": "My Plugin",
  "version": "0.1.0",
  "description": "Optional.",
  "author": "Optional.",
  "apiVersion": 1,
  "providers": [{ "wasm": "provider.wasm" }],
  "permissions": { "read": [], "http": [] },
  "extension": {
    "main": "main.js",
    "styles": ["style.css"],
    "settings": [{ "key": "greet", "label": "Say hello", "type": "toggle", "default": true }]
  }
}
```

| Field | Meaning |
|---|---|
| `id` | 2 to 64 lowercase letters, digits and dashes; unique. |
| `name`, `version` | Shown in Settings > Plugins. |
| `description`, `author` | Optional. |
| `apiVersion` | `1`. |
| `providers`, `permissions` | Stores and what they may reach; see [store plugins](#store-plugins). |
| `extension.main` | The ES module Questkeep imports, relative to the plugin folder. |
| `extension.styles` | Style sheets loaded with the extension (optional). |
| `extension.settings` | Settings the app draws for the plugin (optional); see [settings](#settings). |

Paths are relative, use forward slashes and stay inside the plugin folder. A plugin needs none
of `providers` and `extension` in particular: any combination works.

### Install, reload, remove

Settings > Plugins > "Install from folder…" (or "Install from file…" for a zip) copies the plugin
to `<data>/plugins/<id>/` and remembers where it came from. **Reload** copies the source again
and restarts the plugin; with **Reload on change** on, that happens whenever the source folder
changes. **Disable** stops the plugin and keeps its files; **Remove** deletes the copy and, on
request, the plugin's own data. Nothing needs an app restart.

### Where things are stored

| What | Where |
|---|---|
| The installed copy | `<data>/plugins/<id>/` |
| Which plugins are installed, their source, enabled, reload on change | `<data>/plugins/plugins.json` |
| A store's own files (`/data` inside the sandbox) | `<data>/plugin-data/<id>/` |
| An extension's settings | `<data>/plugin-data/<id>/settings.json` |

`<data>` is the app's data folder (Settings > About has a button that opens it).

## Store plugins

See [`examples/plugins/README.md`](../examples/plugins/README.md): the `Provider` trait, the
sandbox (folders and web hosts a store declares), building for `wasm32-wasip2`.

## Extensions

An extension is an ES module. Questkeep imports `extension.main` into its window and calls
`activate(api)`:

```js
// main.js
/** @type {import("./questkeep-plugin").ExtensionModule["activate"]} */
export function activate(api) {
  api.ui.addTitleBarButton({
    id: "hello",
    label: "Say hello",
    icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="12" cy="12" r="9"/></svg>',
    onClick: () => api.ui.toast(`Hello from ${api.plugin.name}`),
  });
}
```

No build step is needed: plain JavaScript works, other files of the plugin can be imported
(see [hot reload](#developing-with-hot-reload) for a caveat), and
[`questkeep-plugin.d.ts`](../examples/plugins/questkeep-plugin.d.ts) gives completion and type
checking through JSDoc (copy it next to your plugin) or TypeScript. That file is the reference
for the whole API; the host is type-checked against it.

### Lifecycle

1. The plugin is enabled and loaded: the app reads its settings, adds its style sheets and
   imports `main` (always fresh: the URL carries the plugin's load counter).
2. `activate(api)` runs once. It may be async and may return a cleanup function or a
   `{ dispose() }` object.
3. When the plugin is disabled, removed or reloaded: `deactivate()` (if exported), then the
   cleanup `activate` returned, then **everything registered through `api` is removed by the
   app**: buttons, menu entries, pages, settings sections, listeners, styles, open dialogs. A
   reload then starts again at step 1 with the new files.

Every `api.ui.add…` and every `on…` returns a `Disposable`; calling `dispose()` takes that one
thing back earlier. Keeping it is optional.

If the import fails or `activate` throws, the plugin's row in Settings > Plugins says "Extension
failed: …", whatever it had registered is removed, and the app goes on. Errors thrown later (in a
click handler, a listener, a `render`) are logged to the window's console and contained.

### The API

**Plugin**

```js
api.plugin.id; api.plugin.name; api.plugin.version;
img.src = api.plugin.url("images/logo.png");               // a file of the plugin folder
const { helper } = await import(api.plugin.url("lib/helper.js")); // fresh after every reload
```

**Title bar** (left of Settings)

```js
const button = api.ui.addTitleBarButton({
  id: "inbox", label: "Inbox", icon: svgMarkup,
  onClick: (anchor) => api.ui.showMenu(anchor, [{ label: "Mark all read", onSelect: markRead }]),
  badge: () => unread || null, // read again after app events and button.refresh()
});
```

**Menus**: `app` (the Questkeep menu), `add` (the "+" menu), `game` (a game's context menu),
`games` (the context menu of several selected games), `gameMore` (the "more" menu on a game's
page). The function runs each time the menu opens.

```js
api.ui.addMenuItems("game", ({ game }) => [
  { label: "Copy name", icon: svgMarkup, onSelect: () => navigator.clipboard.writeText(game.name) },
  { label: "More", items: [{ label: "Hidden?", checked: game.hidden, onSelect: toggle }] },
  { separator: true },
  { label: "Forget", danger: true, disabled: !game.installed, onSelect: forget },
]);
```

**Pages** (listed in the sidebar under "Plugins") and **dialogs**

```js
const page = api.ui.addPage({
  id: "facts", title: "Library facts", icon: svgMarkup,
  render(container) {
    container.textContent = "…";
    return () => { /* the page was left: stop timers, remove listeners */ };
  },
});
page.open();

const dialog = api.ui.showDialog({
  title: "Pick one", width: "small", // small | medium | large
  render(container, close) { /* draw; call close() when done */ },
});
dialog.close();
api.ui.toast("Saved");           // or api.ui.toast("Failed", "error")
api.ui.addStyles(".mine { … }"); // CSS text; files belong in extension.styles
```

**Settings**

Settings declared in `plugin.json` get a control in Settings, in a section named after the
plugin, without any code:

| `type` | Control | Value | Extra fields |
|---|---|---|---|
| `toggle` | Checkbox | boolean | |
| `text` | Text field | string | |
| `number` | Number field | number | `min`, `max`, `step` |
| `select` | Dropdown | one of the option values | `options: [{ "value", "label" }]` |
| `color` | Color picker and hex field | `#rrggbb` | |

Each has `key`, `label`, optionally `hint` and `default`.

```js
api.settings.get("greet");           // stored value, else the declared default
api.settings.get("lastRun", 0);      // any other key works too (JSON values)
await api.settings.set("lastRun", Date.now());
api.settings.onChange((key, value) => { /* also when the user changes it in Settings */ });
api.ui.addSettingsSection({ id: "preview", title: "Preview", render(container) { /* own UI */ } });
```

**Games, events, theme, commands**

```js
const games = await api.games.list();   // every entry: hidden ones and apps too
const game = await api.games.get(id);
api.games.selected();                    // the selection of the open view
api.games.onSelectionChange((games) => {});
api.games.open(id);                      // shows the game's page

api.events.on("gamesChanged", ({ ids }) => {}); // or "*" for every event
api.theme.scheme();                      // "light" | "dark"
api.theme.onChange(() => {});

await api.invoke("update_game", { id, update: { favorite: true } }); // any app command
api.log.info("ready");                   // console, prefixed with the plugin id
```

Event types: `gamesChanged` (`ids`), `gameRemoved` (`id`), `categoriesChanged`,
`pluginsChanged`, `scansQueued`, `scanStarted`, `libraryScanned` (`provider`, `summary`),
`scanFailed`, `gameStarted` (`id`), `gameStopped` (`id`, `sessionSeconds`), `installFinished`
(`id`), `gameError`, `backgroundWork`, `backupProgress`, `backupFinished`. The commands are
listed in [`ARCHITECTURE.md`](ARCHITECTURE.md#commands-src-taurisrccommandsrs); their arguments
are written in camelCase (`{ providerId }`). `fetch` reaches `https:` URLs and the plugin's own
files.

### Own UI and styling

`render(container)` gets an empty element and fills it with plain DOM, with any library the
plugin ships, or with nothing but `innerHTML`. Return a function to clean up.

The styling contract is the app's CSS classes and its `--qk-*` variables
([`ui/src/theme.css`](../ui/src/theme.css)). Use the variables for every color, size and
spacing and the extension follows every theme:

- classes: `btn`, `btn--primary`, `btn--small`, `btn--danger`, `input`, `check` (a label around a
  checkbox), `muted`, `field__hint`, `form`, `link-btn`, `selectable` (text that can be marked);
- variables: `--qk-color-*` (surface, border, text, accent, danger, …), `--qk-space-1` … `-6`,
  `--qk-radius-*`, `--qk-font-size-*`.

Prefix your own class names (`.my-plugin__thing`): the style sheets share the page with the app.
Icons are SVG markup; use `currentColor` so they follow the text color.

### Trust

Extensions are **trusted code**. They run in the app's window with everything the app's own
interface can do: every command (including starting and removing games), the page's DOM, web
requests to `https:` hosts. There is no sandbox as for stores, and the app's own markup and
class names are not a stable interface beyond what is listed above. Only install extensions you
would run as a program. The install dialog says so.

### Developing with hot reload

1. Install the plugin from its source folder (Settings > Plugins > "Install from folder…").
2. Turn on **Reload on change** in the plugin's row.
3. Edit and save: Questkeep copies the folder again, removes everything the old version
   registered and activates the new one. An open page of the plugin stays open and is drawn by
   the new version.

Every reload gives the plugin's files new URLs (a load counter is part of the path), so `main`,
the style sheets, anything loaded through `api.plugin.url(…)` and the modules they import by a
relative path (`import { x } from "./lib/x.js"`) are all fetched again.

`api.log` output and errors go to the window's console (the developer tools of a dev build).
Keep state that should survive a reload in `api.settings`.
