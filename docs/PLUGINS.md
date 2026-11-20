# Plugins

A plugin is a folder (or a `.zip` of it) with a `plugin.json`. It can bring:

- **stores**: game providers written in Rust, compiled to WebAssembly, run sandboxed;
- an **extension**: JavaScript that adds to the app's interface (title bar buttons, menu
  entries, settings, pages, dialogs);
- **themes**: CSS that restyles the app, with options the user can set.

Examples live in [`examples/plugins`](../examples/plugins):
[`example-store`](../examples/plugins/example-store),
[`example-extension`](../examples/plugins/example-extension) and
[`example-theme`](../examples/plugins/example-theme).

## The plugin package

```json
{
  "id": "my-plugin",
  "name": "My Plugin",
  "version": "0.1.0",
  "description": "Optional.",
  "author": "Optional.",
  "icon": "icon.svg",
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
| `icon` | Optional. An `.svg` or `.png` file inside the plugin folder (square; at most 200 KB), shown at the start of the plugin's row in Settings > Plugins and in the install dialog. An SVG may not contain `<script`, `on<event>=` attributes or `javascript:` links, and is only ever shown as an image. Install refuses a missing, too large or unsafe icon with a message. Questkeep versions before this field ignore it, so a plugin that adds an icon needs no `minApp`. |
| `apiVersion` | `1`. |
| `providers`, `permissions` | Stores and what they may reach (`read`, `http`, `folders`); see [store plugins](#store-plugins). |
| `permissions.folders` | Folders the user chooses for the plugin; see [folders the user chooses](#folders-the-user-chooses). |
| `extension.main` | The ES module Questkeep imports, relative to the plugin folder. |
| `extension.styles` | Style sheets loaded with the extension (optional). |
| `extension.settings` | Settings the app draws for the plugin (optional); see [settings](#settings). |
| `themes` | Themes the plugin brings (optional); see [themes](#themes). |

Paths are relative, use forward slashes and stay inside the plugin folder. A plugin needs none
of `providers`, `extension` and `themes` in particular: any combination works.

### Install, reload, remove

Settings > Plugins > "Install from folder…" (or "Install from file…" for a zip) copies the plugin
to `<data>/plugins/<id>/` and remembers where it came from. **Reload** copies the source again
and restarts the plugin; with **Reload on change** on, that happens whenever the source folder
changes. **Disable** stops the plugin and keeps its files; **Remove** deletes the copy and, on
request, the plugin's own data. Nothing needs an app restart.

Plugins can also be installed from the community plugin registry (Settings > Plugins > "Browse plugins…", the plugin
browser): the zip is downloaded from the author's release, checked against the hash the registry
lists, shown in the usual install confirmation and then installed. Such a plugin remembers its
registry version (updates are offered unless you picked an older version on purpose, which
"pins" it); before an update or downgrade replaces it, its data folder is copied to
`<data>/plugin-data-backup/<id>/<old version>/` (the last 3 are kept). Plugins installed from a
folder or file are never touched by the registry. Format and flow: [`PLUGIN_REGISTRY.md`](PLUGIN_REGISTRY.md).

Questkeep looks for updates of registry plugins about 20 seconds after start and every 12 hours
(Settings > Plugins: "Check for plugin updates" turns it off, "Check now" asks right away) and puts
one entry in the notification bell when there is a new version. The Plugin updates dialog (bell
entry or Settings > Plugins > Updates) updates one plugin or all of them; "Update all" leaves out
updates that ask for more than the installed version (new web hosts, folders, an extension or
store) so you can look at them one by one. Nothing is installed without your go-ahead.

### Where things are stored

| What | Where |
|---|---|
| The installed copy | `<data>/plugins/<id>/` |
| Which plugins are installed, their source, enabled, reload on change, the folders the user chose | `<data>/plugins/plugins.json` |
| A store's own files (`/data` inside the sandbox) | `<data>/plugin-data/<id>/` |
| An extension's settings | `<data>/plugin-data/<id>/settings.json` |
| Registry plugins: the downloaded zip (the plugin's `source`) | `<data>/plugin-cache/<id>/<version>.zip` |
| Registry plugins: data copies made before an update or downgrade | `<data>/plugin-data-backup/<id>/<old version>/` |
| The registry's last loaded index and icons | `<data>/plugins/registry-cache.json`, `registry-icon.<id>.json` |
| The bell entry for plugin updates and the update pairs the user dismissed | `<data>/plugins/plugin-update-notices.json` |

`<data>` is the app's data folder (Settings > About has a button that opens it).

## Store plugins

See [`examples/plugins/README.md`](../examples/plugins/README.md): the `Provider` trait, the
sandbox (folders and web hosts a store declares), building for `wasm32-wasip2`.

### Folders the user chooses

`permissions.read` mounts folders at fixed places. A plugin whose files live somewhere the user
decides (a game manager's instances folder, a library of ROMs) declares a **folder** instead:

```json
"permissions": {
  "folders": [
    { "key": "instances", "label": "BSManager instances folder",
      "hint": "The folder that contains your Beat Saber instances.",
      "as": "/instances", "default": "~/BSManager/BSInstances" }
  ]
}
```

| Field | Meaning |
|---|---|
| `key` | 1 to 32 lowercase letters, digits and dashes; unique in the plugin. Names the choice in `plugins.json`. |
| `label` | Required. What the folder is; shown in the install dialog and Settings > Plugins. |
| `hint` | Optional. One sentence of help under the label. |
| `as` | Where the stores see it, validated like `permissions.read` names (`/instances`, not `/plugin` or `/data`). Unique across `read` and `folders`. |
| `default` | Optional. Used until the user chooses; `%VAR%` and a leading `~` are expanded like in `read` paths. |

- The folder a store gets is the user's choice, else the expanded `default`. It is mounted
  **read-only** at `as`, and only when it exists and is a folder; otherwise nothing is mounted
  (the store sees no `/instances`, like a missing `read` folder), and Settings > Plugins shows
  "This folder was not found".
- The **install dialog** lists each folder (label, hint, the default, "not found" when it is not
  there) with **Choose…** and **Use default**, so the folders are set before anything is loaded
  and the first scan already sees them. An update's dialog starts with the current choice. A
  folder that is not found does not block the install (the user may fix it later). If a folder
  is still unset and its default is missing after the install, the app does not scan the
  plugin's stores (that would only fail) and shows "Choose <label> in Settings > Plugins >
  <plugin> > Folders to list its games." Afterwards, Settings > Plugins shows a **Folders**
  block per plugin with the same buttons. Changing a folder there reloads the plugin's stores
  and scans them again.
- The install commands (`install_plugin`, `install_registry_plugin`) take the dialog's choices
  as an optional `folders` map (key to path). Every entry is checked like a later choice (below)
  before anything changes: one bad entry refuses the whole install and an installed copy
  stays. The choices are saved with the install, before the plugin is loaded. Choices made
  earlier stay unless the map names the key again (a blank path goes back to the default).
- The user can only pick an existing folder, given as a full path, that is neither Questkeep's
  own data folder (or a folder containing it: it holds the library database) nor the plugin's
  installed folder.
- The choice is stored in `<data>/plugins/plugins.json` (`folders`: key to path), **never** in
  `<data>/plugin-data/<id>/`, which the plugin can write: a plugin cannot choose or change a
  path itself, only read what the user gave it. An update, a reinstall (also from another
  source) and a version switch keep the choice; Remove deletes it.
- Older Questkeep versions ignore `folders` without a message, so the plugin would start there
  without the folder. A plugin that needs it must set `minApp` in the
  [registry](PLUGIN_REGISTRY.md) to the first Questkeep version with this feature (the release
  that added `permissions.folders`; `pnpm changeset status` names it while it is pending).
  `apiVersion` stays `1`.

#### Paths below a mount

A store that found a game in `/instances/My Instance/Beat Saber.exe` returns that path as is:
Questkeep translates a path that starts with the guest path of a `read` or `folders` mount to
the real path on this PC (`D:\BSManager\BSInstances\My Instance\Beat Saber.exe`) in
`install_dir`, in play actions (`target` of a `file` action, `working_dir`, `tracking_path` when
the tracking mode is `directory`; also install, uninstall, launcher-page and launch-with-args
actions and the store launcher) and in image candidates (`file`). Rules:

- Only whole path parts match, case-sensitively, like the sandbox itself: `/instances/a` and
  `/instances` translate, `/instancesx` and `/Instances` do not. The longest matching mount wins.
- `.` parts, empty parts and trailing slashes are dropped; a path with a `..` part is **not**
  translated and stays as written (it can never leave a mount).
- Paths that are not below a mount stay as they are, e.g. a real `C:\Games\x.exe` or a URI.
- `/plugin` and `/data` are never translated for launching or install dirs: a plugin writes
  `/data`, so a path there must not turn into a program Questkeep starts. (Image candidates may
  still come from them, since images are only read.)

## Themes

A plugin can bring any number of themes. Each is an entry in `themes` and a CSS file:

```json
{
  "id": "my-themes",
  "name": "My Themes",
  "version": "0.1.0",
  "apiVersion": 1,
  "themes": [
    {
      "id": "my-themes-dusk",
      "name": "Dusk",
      "description": "Optional.",
      "css": "dusk.css",
      "params": [
        { "key": "accent", "label": "Accent color", "type": "color",
          "default": { "light": "#0f6f96", "dark": "#6cc5e8" } }
      ]
    }
  ]
}
```

The theme's `id` must be the plugin's id or start with it and a dash. While the plugin is enabled
its themes are listed in Settings > Appearance, marked "Plugin: <plugin name>"; their options
(`params`) show below the picker. With **Reload on change** on, saving the CSS file updates the
app at once, also while the theme is the chosen one. A theme that cannot be loaded (a missing
file, an id that is taken) is reported in the plugin's row in Settings > Plugins.

Themes are plain CSS and cannot run code, but they can restyle and rearrange everything, so a
theme can also hide or disguise parts of the app. The whole format (variables, parameters,
files a theme ships, limits) is in [`docs/THEMES.md`](THEMES.md);
[`example-theme`](../examples/plugins/example-theme) is a complete theme with options and a
layout change. The quickest way to a theme plugin is the theme editor's "Export as plugin…".

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
