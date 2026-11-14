# Plugins: stores, extensions, themes

## Intent

Questkeep gets plugins that people can write, try and share without rebuilding the app. A plugin
can bring game providers (stores), extensions (menu entries, settings, title bar buttons, own UI)
and themes. Writing one must be pleasant: load from a folder, change the source, reload, unload,
all without restarting the app. Themes get a live editor, parameters users can adjust, and the app
ships more themes, each with a light and a dark mode.

Decided by the user: providers run as sandboxed WASM; extension JS is trusted and runs in the main
webview; work lands on three stacked local branches (`feat/plugins-providers` ->
`feat/plugins-extensions` -> `feat/plugins-themes`), nothing is pushed. Everything else below is
my call.

## Goals / non-goals

- Goals: one plugin format for all three kinds; install = copy into the data folder, remembering
  the source, so Reload re-copies; load/unload/reload at runtime; an example plugin per kind that
  is used for testing; built-in themes with light + dark; theme parameters (few basic, rest
  "advanced"); live theme editor.
- Non-goals (roadmap): a plugin gallery/download, signature checks, account sign-in for plugin
  stores, Windows registry access for WASM stores, theming the native tray popup.
- Anti-goals: a plugin crashing or hanging the app; a WASM store reading files or hosts it did not
  declare; a restart being needed for anything.

## Approaches considered

- **Providers:** WASM component (chosen: sandbox, safe unload, matches `docs/SPEC.md`), embedded
  JS engine (fast loop but a wide host API and slow), external process (no sandbox).
- **Provider interface:** a fully typed WIT mirror of `LibraryProvider` (every model change breaks
  the ABI) versus a tiny WIT (`describe` + `call(method, json)`) with a typed Rust SDK on top
  (chosen: the SDK shares the serde model types with the host, the ABI stays stable).
- **Extension UI:** trusted JS in the webview (chosen by the user) versus sandboxed iframes.
- **Themes:** CSS variable overrides plus free CSS (chosen: covers "just colors" and "layout
  overhaul" with one mechanism) versus a JSON-only token format (no layout changes possible).

## The plugin package

A folder (or a `.zip` of it) with `plugin.json`:

```json
{
  "id": "example-store",            // [a-z0-9-], unique
  "name": "Example Store",
  "version": "0.1.0",
  "description": "…", "author": "…",
  "apiVersion": 1,
  "providers": [{ "wasm": "provider.wasm" }],
  "permissions": {
    "read": [{ "path": "%LOCALAPPDATA%/ExampleStore", "as": "/store" }],
    "http": ["api.example.com"]
  },
  "extension": { "main": "main.js", "settings": [ … ] },      // branch 2
  "themes": [{ "id": "…", "name": "…", "css": "theme.css", "params": [ … ] }]   // branch 3
}
```

- Installing copies the package to `<data>/plugins/<id>/` and records `{ id, source, enabled }`
  in `<data>/plugins/plugins.json`. **Reload** copies again from `source` (when it still exists)
  and loads the new copy. **Disable** unloads and keeps the files; **Remove** unloads and deletes
  the copy (the plugin's own data in `<data>/plugin-data/<id>/` stays unless asked).
- A developer toggle **Watch source** reloads the plugin whenever its source folder changes.
- Enabled plugins load at startup; a plugin that fails to load is listed with its error.

## Branch 1: provider plugins

- `questkeep-store-model` (new crate): `model.rs`, `settings.rs` and `AddGameOption` move out of
  `questkeep-store-api` (which re-exports them) so the WASM guest can use the very same types.
- `questkeep-plugin-sdk` (guest, Rust, `wasm32-wasip2`): trait `Provider` mirroring
  `LibraryProvider` minus accounts, `export_provider!`, host helpers (`log`, `http`).
- WIT world `questkeep:plugin/provider@1`: exports `describe() -> string` and
  `call(method, args-json) -> result<string, provider-error>`; imports `host.log` and
  `host.http-request`. Files come through WASI preopens: `/plugin` (read-only, the installed
  copy), `/data` (read-write, the plugin's own folder), plus the declared `read` mounts
  (read-only, `%VAR%` expanded). HTTP only reaches the declared hosts, through the app's `ureq`.
- `questkeep-plugins` (host crate; depends on core, the shell depends on it): manifest, install,
  state, `PluginManager` (load/unload/reload/watch), `WasmProvider: LibraryProvider`. One
  instance per provider behind a mutex; cheap answers (`id`, `name`, icon, capabilities,
  launcher, settings) come from the cached `describe` result; a trap discards the instance and
  the next call makes a new one; an epoch deadline stops runaway calls.
- Core: `ProviderRegistry` becomes shared and changeable at runtime (`insert`/`remove` through
  `&self`; clones share one map), `LibraryProvider::icon_svg`/`shop_url` return `&str`, secret
  provider settings are looked up from the live registry, the metadata resolver follows the
  registry, and `CoreEvent::PluginsChanged` tells the UI to re-read providers and plugins.
- Unloading a store leaves its games in the library (they behave like games of any missing
  store) and they come back to life when the plugin is loaded again.
- Shell commands: `list_plugins`, `install_plugin(path)`, `reload_plugin`, `set_plugin_enabled`,
  `remove_plugin`, `set_plugin_watch`. UI: a **Plugins** settings section (list with kind badges,
  version, error, Reload / Disable / Remove, "Install from folder…", "Install from file…").
- Example: `examples/plugins/example-store` (Rust -> wasm) reading `games.json`.

## Branch 2: extensions

- The shell serves installed plugin files through a `qkplugin` URI scheme (only files below
  `<data>/plugins/<id>/`); the CSP allows scripts and styles from it.
- The UI imports `extension.main` as an ES module (cache-busted per load) and calls
  `activate(api)`. Everything a plugin registers goes through `api` and is tracked per plugin, so
  unloading disposes it all; `deactivate()` is called when present.
- `api`: `plugin` (id, version, `url(path)`), `ui.addTitleBarButton`, `ui.addMenuItems(location)`
  (`app`, `add`, `game`, `games`, `gameMore`), `ui.addSettingsSection`, `ui.addPage` (a sidebar
  page), `ui.showDialog`, `ui.toast`, `settings.get/set/onChange` (stored under
  `settings.plugins.<id>`), `games.list/get/selected`, `events.on`, `invoke(command, args)`.
  Own UI is plain DOM: `render(container)` returning a dispose function; the app's CSS classes
  and `--qk-*` variables are the styling contract.
- Declared settings (`extension.settings`: toggle, text, number, select, color) are drawn by the
  app in the plugin's settings section without any plugin code.
- Example: `examples/plugins/example-extension`.

## Branch 3: themes

- `Settings.appearance`: `{ theme, scheme: system | light | dark, params: { <theme>: {…} } }`.
  The UI sets `data-scheme` and `data-theme` on `<html>`; `theme.css` gains a light palette.
- A theme is CSS that overrides `--qk-*` variables (and may restyle anything), scoped by the app
  to `data-theme`, with both schemes. Parameters (`color`, `number` with unit, `select`,
  `toggle`; `advanced: true` hides them behind "Advanced") become CSS variables the theme uses.
  The default theme exposes the accent color, corner roundness, density and font size.
- Built-in themes ship in the UI in the same format as plugin themes.
- **Theme editor**: pick a base theme, change any variable with live preview (color pickers,
  grouped, searchable), per scheme; save as a user theme (a generated plugin in the data
  folder), export as a plugin folder.
- Example: `examples/plugins/example-theme` (also contains a layout change).

## Constraints

AGENTS.md conventions; third-party packages at least 7 days old and license-compatible with MIT;
UI verified in the hidden test app (`target/agent`, Vite :1421, DevTools :9223), never launching
games or touching sign-ins there.
