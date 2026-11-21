# Questkeep plugins

A plugin is a folder (or a `.zip` of it) with a `plugin.json`. The guide to the package is
[`docs/PLUGINS.md`](../../docs/PLUGINS.md). The examples here, one per kind of plugin:

| Example | What it is | Guide |
|---|---|---|
| [`example-store`](example-store) | A store: Rust compiled to WebAssembly, run sandboxed, reading its games from `games.json` | this page |
| [`example-extension`](example-extension) | An extension: plain JavaScript that adds a title bar button, menu entries, a page, a dialog and settings ([`questkeep-plugin.d.ts`](questkeep-plugin.d.ts) is its API) | [`docs/PLUGINS.md`](../../docs/PLUGINS.md#extensions) |
| [`example-theme`](example-theme) | A theme: CSS with a light and a dark palette, options and a layout change | [`docs/THEMES.md`](../../docs/THEMES.md) |

The rest of this page covers store plugins: stores written in Rust, compiled to WebAssembly and
run sandboxed by Questkeep.

## plugin.json

```json
{
  "id": "my-store",
  "name": "My Store",
  "version": "0.1.0",
  "description": "Optional.",
  "author": "Optional.",
  "icon": "icon.svg",
  "apiVersion": 1,
  "providers": [{ "wasm": "provider.wasm" }],
  "permissions": {
    "read": [{ "path": "%LOCALAPPDATA%/MyStore", "as": "/store" }],
    "folders": [{ "key": "library", "label": "My Store library folder", "as": "/library" }],
    "http": ["api.example.com", "*.example-cdn.com"]
  }
}
```

- `id`: 2 to 64 lowercase letters, digits and dashes. The store's id must be the same, or start
  with it and a dash (`my-store-beta`).
- `icon` (optional): an `.svg` or `.png` in the plugin folder, at most 200 KB; an SVG may not
  contain scripts, `on...=` attributes or `javascript:` links. It is shown in Settings > Plugins
  and the install dialog. Older Questkeep versions ignore the field, so it needs no `minApp`.
- `providers[].wasm`: the built component, relative to the plugin folder.
- `permissions.read`: folders the store may read, each mounted read-only at its `as` path.
  `%VAR%` and a leading `~` are expanded; a folder that does not exist is simply not mounted.
- `permissions.folders`: folders the **user** chooses (Settings > Plugins > Folders). Each has a
  `key` (1 to 32 lowercase letters, digits, dashes; unique), a `label`, an optional `hint` and
  `default` (expanded like `read` paths), and an `as` path that is unique across `read` and
  `folders`. The effective folder (the choice, else the default) is mounted read-only at `as`
  when it exists. The store can neither choose nor change it. Older Questkeep versions ignore
  the field: set `minApp` in the plugin registry. Details: [`docs/PLUGINS.md`](../../docs/PLUGINS.md#folders-the-user-chooses).
- Paths: a path the store returns that starts with the guest path of a `read` or `folders` mount
  (`/library/Game/game.exe` as `install_dir`, a play action's `target`/`working_dir`, an image
  `file`) is translated to the real path on the PC; other paths stay as they are, `..` is never
  resolved, and `/plugin` and `/data` never become launchable paths.
- `permissions.http`: hosts the store may request over https. `*.example.com` covers every
  subdomain; `http://localhost` also allows plain http for that host. Redirects are not followed.
- `permissions.filesystem: "read"`: every file of the PC, read-only, below `/host` (`C:\Games` is
  `/host/c/Games`, `/home/me` is `/host/home/me`). Use `questkeep_plugin_sdk::known_folder("…")`
  to find usual places (`local-app-data`, `program-data`, `config`, …) and
  `questkeep_plugin_sdk::host_path(r"C:\Program Files\Steam")` to turn a path read from the
  registry or a config file into the plugin's view of it. `sqlite::query(path, sql, params)` reads
  other programs' SQLite databases (read-only; locked databases work).
- `permissions.registry: "read"`: `registry::read`, `registry::subkeys`,
  `registry::uninstall_entries()` and `registry::appx_packages()` (Windows; empty elsewhere).
- Secrets: `secrets::get/set/delete` keep tokens in the system keyring under the plugin's name
  (no permission needed). Keep tokens there, not in `/data`.
- `permissions.launcher.processNames`: the launcher's executable names; only these can be closed
  after a game (see `Provider::launcher_process`).
- `permissions.login` with `has_account`, `account_status`, `sign_in_with_code` and `sign_out`:
  a sign-in window locked to the declared `domains`, an optional website session
  (`session::refresh()`). Details and the manifest block:
  [`docs/PLUGINS.md`](../../docs/PLUGINS.md#accounts-and-sign-in).

A store that declares `filesystem` or `registry` can read everything the user can; the web hosts
it lists are the only way for that to leave the PC and are shown in the install dialog, and the
registry reviews them. The test store [`sandbox_probe`](../../crates/questkeep-plugin-sdk/examples/sandbox_probe.rs)
uses each of these and is what the app's plugin tests run.

## Writing a store

A cargo library with `crate-type = ["cdylib"]` that depends on `questkeep-plugin-sdk`:

```rust
use questkeep_plugin_sdk::model::ImportedGame;
use questkeep_plugin_sdk::{Provider, ProviderError, export_provider};

#[derive(Default)]
struct MyStore;

impl Provider for MyStore {
    fn id(&self) -> String {
        "my-store".into()
    }
    fn name(&self) -> String {
        "My Store".into()
    }
    fn is_installed(&self) -> bool {
        std::path::Path::new("/store").is_dir()
    }
    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        Ok(vec![ImportedGame::installed("1", "First Game")])
    }
}

export_provider!(MyStore);
```

`Provider` has the same methods as Questkeep's built-in stores (scans, install and uninstall
actions, metadata, news, achievements, covers, store options, ...), all with defaults. What the
store can use:

- `std::fs` on `/plugin` (the installed plugin folder, read-only), `/data` (the plugin's own
  folder, read-write, kept across updates) and the declared `read` mounts. Paths the store
  returns for launching games must be real paths on the PC, not mount paths; image files may be
  given as mount paths.
- `questkeep_plugin_sdk::http::{get, request}` for the declared hosts.
- `questkeep_plugin_sdk::log::{debug, info, warn, error}` for Questkeep's log (a panic is
  logged too).

There is no environment, no sockets and no sign-in. A call that takes longer than 120 seconds
(scans, metadata, images; 10 seconds for everything else) or crashes is stopped and reported;
the store starts fresh with the next call. Store options (`settings`) are switches; their values
arrive in `configure` before every sync.

## Build

```sh
rustup target add wasm32-wasip2   # once
cargo build --release --target wasm32-wasip2
```

Copy `target/wasm32-wasip2/release/<crate_name>.wasm` to the path named in `plugin.json`
(`example-store/build.ps1` does both).

## Install and reload

In Questkeep: Settings > Plugins > "Install from folder…" (or "Install from file…" for a zip).
Questkeep copies the plugin into its data folder and remembers where it came from. After
changing the plugin, build it and press Reload: the folder is copied again and the store starts
anew, without restarting the app. With "Watch source" on, that happens whenever the folder
changes (build output in `target/` is ignored). Disable unloads the store and keeps the files;
Remove deletes the copy, and on request the plugin's own data. Games of an unloaded store stay
in the library.
