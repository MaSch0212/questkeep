# Example Store

A Questkeep store plugin that reports the games listed in `games.json`. It shows the whole
provider interface in a few lines: scanning, a store option, metadata, covers and logging.

## Build

```powershell
rustup target add wasm32-wasip2   # once
./build.ps1                       # writes provider.wasm next to plugin.json
```

On other systems: `cargo build --release --target wasm32-wasip2`, then copy
`target/wasm32-wasip2/release/example_store.wasm` to `provider.wasm`.

## Try it

Install this folder in Questkeep (Settings > Plugins > Install from folder). Change `games.json`
or the code, build again and press Reload; with "Watch source" on, Questkeep reloads by itself.

## games.json

A list of games:

| Field | Meaning |
|---|---|
| `id`, `name` | Required. |
| `exe` | The executable to start. |
| `installDir` | The game's folder. |
| `installed` | `false` for a game that is owned but not installed. |
| `playtimeSeconds` | Playtime known to the store. |
| `demo` | Only reported while the store option "Include demos" is on. |
| `description`, `genres`, `developers`, `releaseDate` | Shown on the game page. |
| `cover` | URL of a portrait cover. |
