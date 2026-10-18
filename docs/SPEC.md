# Questkeep v0.1 Specification

Questkeep is an open-source (MIT) desktop game library manager and launcher: a lightweight,
extensible alternative to Playnite. This document is the agreed spec from the v0.1 brainstorm
(2026-10-18).

## 1. Project facts

| Item | Value |
|---|---|
| Repository | `MaSch0212/questkeep` on GitHub, public |
| Description | Lightweight, extensible game library manager and launcher (Tauri + Rust + SolidJS) |
| License | MIT, `Copyright (c) 2026 Marc Schmidt` |
| Third-party notices | Playnite and PlayniteExtensions (MIT, Josef Nemec) in `THIRD_PARTY_NOTICES.md` |
| Commits | Conventional Commits, small commits straight to `main`, push after each slice |

## 2. Platforms

- **Windows 10/11** and **Linux** must both work in v0.1.
- OS-specific code (registry, process enumeration, path discovery) lives behind a `platform`
  module in `questkeep-core` using `#[cfg(windows)]` / `#[cfg(target_os = "linux")]`, with
  stubs for other targets so the core compiles everywhere.
- Steam detection:
  - Windows: registry `HKCU\Software\Valve\Steam\SteamPath` (fallback `HKLM\SOFTWARE\WOW6432Node\Valve\Steam\InstallPath`).
  - Linux: `~/.steam/steam`, `~/.local/share/Steam`, `~/.var/app/com.valvesoftware.Steam/.local/share/Steam` (Flatpak).
- Process tracking: Win32 toolhelp snapshot on Windows, `/proc` on Linux.
- CI runs full gates + build on both `windows-latest` and `ubuntu-latest`.

## 3. Stack

- Tauri 2 shell, Rust stable Cargo workspace.
- `questkeep-core` crate with no Tauri/UI dependency; `src-tauri` is a thin command + event layer.
- SolidJS + TypeScript + Vite in `ui/`, plain CSS with custom properties, `@tanstack/solid-virtual`.
- SQLite via `rusqlite` (bundled), ordered migrations with a `schema_version` table.
- Shared TS types generated from Rust (`ts-rs` or `tauri-specta`; choice recorded in DECISIONS.md).
- Quality gates: rustfmt, clippy (`-D warnings`, `[workspace.lints]`), cargo test, oxfmt, oxlint,
  `tsc --noEmit` (strict), vitest. One aggregate `check` command.

## 4. Data model

- `Game`: UUID id, `name`, `sorting_name`, `source` (provider id, e.g. `steam`, `manual`),
  `source_game_id` (store id; unique together with `source` for dedup on re-scan),
  `install_dir`, `play_action`, `playtime_seconds`, `last_played`, `date_added`, `favorite`,
  `hidden`, `cover_path`, `icon_path`, `extra` (free-form JSON for future fields).
- `play_action` is a typed object stored as JSON:
  `{ kind: "file" | "uri", target, args, working_dir, tracking: "default" | "process" | "directory" | "name", tracking_path }`.
  v0.1 edits one action per game; later versions may grow this into a list (emulators, extra actions).
- No Playnite library import is planned; the model owes no compatibility to Playnite's.

## 5. Data location

- Default: the OS app-data directory (`%APPDATA%\questkeep` on Windows, `$XDG_DATA_HOME/questkeep`
  on Linux).
- Portable mode: if a file named `portable.txt` exists next to the executable, data goes to a
  `data/` folder next to the executable instead.
- Contents: one SQLite file, `images/` cache, `logs/`, `settings.json` (or settings table).

## 6. Extensibility direction

- Future plugins: sandboxed **WASM** (wasmtime + WIT) for providers and logic; **CSS/JS** in the
  webview for UI themes and widgets.
- Therefore in v0.1:
  - `LibraryProvider` trait is object-safe, takes/returns plain serde-serializable structs
    (no Tauri, rusqlite or OS handle types). Methods: `id()`, `name()`, `scan_installed()`,
    room for `scan_owned()` later. A registry looks providers up by id; adding a provider does not
    touch UI or DB code.
  - Event bus events are a serde-serializable enum (game added/updated/removed, started, stopped,
    library scanned). The Tauri shell forwards them to the UI.
  - All visual values in one `theme.css` via CSS variables; dark default theme; no loader yet.

## 7. v0.1 feature scope

**Library**: grid of covers (virtualized, smooth at 5,000+ games); as-you-type case-insensitive
search; sort by name, last played, playtime, date added; filter by source, favorites, hidden;
details panel with a prominent Play button, editable name, favorite and hide toggles, remove game.

**Adding games**
- Manual: pick an executable, name auto-filled from file/folder name, optional args and cover image.
- Steam installed games: detect install path, parse `libraryfolders.vdf` and every
  `appmanifest_*.acf` across library folders, skip tools/runtimes as Playnite does, launch via
  `steam://rungameid/<id>`, re-scan updates instead of duplicating. Well-tested VDF/ACF parser.
- Stretch only: Epic installed games (Windows manifests under ProgramData).

**Launching and playtime**: launch, track process (and children where reasonable), add session
duration to playtime, update last played, emit started/stopped. URI-launched games are detected by
processes whose executable lives under the install dir. No orphaned tracker tasks; app exit during
a session saves elapsed time.

**Images**: Steam covers from Steam's CDN, on demand, cached on disk with bounded concurrency and
fallbacks; served via Tauri's asset protocol; placeholder tile when no cover exists.

**Settings**: view to trigger Steam rescan and show the data directory.

**Persistence**: survives restarts.

## 8. Look and feel

Clean dark theme: neutral dark surfaces, one accent color, 2:3 portrait covers in a virtualized
grid, left sidebar for sources and filters, top bar with search and sort, slide-in details panel
with a big Play button. Keyboard navigation (arrows, Enter opens, shortcut launches), visible focus
states, labeled buttons.

## 9. Non-goals (v0.1)

Fullscreen/controller mode, plugin loading, theme loading, metadata scrapers, GOG/Origin/Ubisoft/
Battle.net/Xbox providers, emulators, cloud sync, achievements, social, auto-update, release
automation, installers beyond default `tauri build` output, Playnite import.

## 10. Provider roadmap

Steam (v0.1), Epic (v0.1 stretch), then GOG, Xbox, others.

## 11. Quality bar

Unit tests for VDF/ACF parsing (realistic fixtures), provider dedup, migrations, playtime
accumulation, sort/filter queries; vitest for frontend logic and key components. No `unwrap()`/
`expect()` in non-test code that touches user data or the filesystem (`thiserror` errors, readable
UI errors). `tracing` with a log file in the data dir. Dev-only seed of 5,000 fake games; measured
idle memory and cold start recorded in `docs/ARCHITECTURE.md`.

## 12. CI

`.github/workflows/ci.yml`: push to `main` + pull requests, concurrency cancel, `contents: read`.
Windows and Ubuntu jobs run all gates and a Tauri build; Windows uploads build output as an
artifact (7-day retention). No releases or tag triggers.
