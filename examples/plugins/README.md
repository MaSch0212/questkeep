# Questkeep plugins

A plugin is a folder (or a `.zip` of it) with a `plugin.json`. The guide to the package and to
extensions (JavaScript that adds to the app's interface) is
[`docs/PLUGINS.md`](../../docs/PLUGINS.md); [`example-extension`](example-extension) is a
complete extension and [`questkeep-plugin.d.ts`](questkeep-plugin.d.ts) its API.

This page covers store plugins: stores written in Rust, compiled to WebAssembly and run
sandboxed by Questkeep. [`example-store`](example-store) is a complete one.

## plugin.json

```json
{
  "id": "my-store",
  "name": "My Store",
  "version": "0.1.0",
  "description": "Optional.",
  "author": "Optional.",
  "apiVersion": 1,
  "providers": [{ "wasm": "provider.wasm" }],
  "permissions": {
    "read": [{ "path": "%LOCALAPPDATA%/MyStore", "as": "/store" }],
    "http": ["api.example.com", "*.example-cdn.com"]
  }
}
```

- `id`: 2 to 64 lowercase letters, digits and dashes. The store's id must be the same, or start
  with it and a dash (`my-store-beta`).
- `providers[].wasm`: the built component, relative to the plugin folder.
- `permissions.read`: folders the store may read, each mounted read-only at its `as` path.
  `%VAR%` and a leading `~` are expanded; a folder that does not exist is simply not mounted.
- `permissions.http`: hosts the store may request over https. `*.example.com` covers every
  subdomain; `http://localhost` also allows plain http for that host. Redirects are not followed.

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
