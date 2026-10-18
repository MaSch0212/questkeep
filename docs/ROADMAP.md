# Roadmap

Ideas deliberately left out of v0.1, roughly in priority order. Not a commitment.

## Next (v0.2)

- **Epic Games provider** (installed games from `%PROGRAMDATA%\Epic\EpicGamesLauncher\Data\Manifests`).
- **GOG provider** (registry `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games` / Galaxy database), then
  **Xbox / Microsoft Store**.
- **Steam owned (not installed) games** via `scan_owned()` and the Steam Web API, with an
  "Install" action (`steam://install/<id>`).
- **Import Steam playtime and last played** from `localconfig.vdf`.
- **Metadata**: descriptions, genres, release dates, backgrounds (store APIs first, IGDB later).
- **List view** as an alternative to the cover grid.

## Later

- Plugin host: WASM providers (wasmtime + WIT) and CSS/JS UI extensions; theme loader on top of
  `theme.css` variables.
- Fullscreen / controller mode (the grid keyboard navigation is the groundwork).
- Multiple play actions per game, emulator profiles, pre/post scripts.
- Roman numeral support in sorting names (Playnite's converter handles many edge cases).
- Tray icon, minimize while playing.
- Auto-update and release automation (signed installers).
- Cloud sync of the library database.
