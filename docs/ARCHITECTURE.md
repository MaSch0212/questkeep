# Architecture

Questkeep is a Tauri 2 app. All logic lives in the `questkeep-core` crate, which has no
dependency on Tauri or any UI. `src-tauri` is a thin shell that exposes typed commands and
forwards core events to the webview. The SolidJS UI in `ui/` talks to the core only through
those commands and events.

```
 ┌────────────── ui/ (SolidJS) ───────────────┐
 │ components ─ stores ─ lib/api.ts (invoke)  │
 └───────────────▲───────────────┬────────────┘
       events    │               │ commands (typed, ts-rs bindings)
 ┌───────────────┴─ src-tauri ───▼────────────┐
 │ commands.rs (thin) · event forwarding      │
 └───────────────▲───────────────┬────────────┘
                 │ EventBus      │ Library / Launcher API
 ┌───────────────┴ questkeep-core ▼───────────┐
 │ library · db · providers · launcher ·      │
 │ images · settings · events · platform      │
 └────────────────────────────────────────────┘
```

## Modules (`crates/questkeep-core/src`)

| Module | Responsibility |
|---|---|
| `paths` | Resolves the data directory (app-data or portable `data/` next to the exe when `portable.txt` exists). |
| `error` | `CoreError` (`thiserror`), serialized to the UI as a readable message. |
| `model/` | `Game`, `PlayAction`, `ActionKind`, `TrackingMode`, `ImportedGame`, `NewManualGame`, `GameUpdate`, `GameQuery`, `SortField`. All `serde` + `ts-rs`. |
| `db/` | `Database` (rusqlite connection behind a mutex), ordered migrations with `schema_version`, `games` repository (CRUD, query, upsert of imported games), `sessions` repository. |
| `providers/` | `LibraryProvider` trait, `ProviderRegistry`, `steam` (VDF/ACF parser, library discovery, tool filter), `manual` helpers, `epic`/`gog` (Windows launchers; Heroic/Legendary on Linux through the shared `heroic` module), `xbox`, `lutris` (Linux, reads `pga.db`). |
| `launcher/` | Starts games (`file` spawn or `uri` open), tracks sessions on a background thread per game, writes playtime. `ProcessProbe` trait keeps tracking testable. |
| `images/` | Image cache under `images/covers` and `images/backgrounds`. `CoverFetcher` (4 workers) tries each provider's cover candidates, local files before CDN URLs. `MetadataFetcher` (1 worker, at least 1.5 s between store requests) stores store metadata and a background per game. `SteamGridDb` (`images/steamgriddb.rs`) is a fallback source for covers and backgrounds when a user API key is set: used when the provider has no image (or only low-quality ones, see `covers_are_low_quality`), and for manual games. Global limiter of 2 requests/s, "nothing found" remembered in `games.extra`, key shared via `Library::artwork()` and updated by `save_settings`. |
| `settings` | Small `settings.json` (serde, defaults on missing/corrupt file), including a per-provider config map (`providers.steam = { apiKey, steamId }`). |
| `events` | `EventBus` with serializable `CoreEvent` enum; subscribers are callbacks. Plugin hooks attach here later. |
| `platform/` | OS-specific bits: Steam install dirs (registry on Windows, XDG paths on Linux), opening URIs, process snapshots. Stubs on other targets. |
| `library` | `Library` facade used by the shell: wires db, providers, images, events. |
| `seed` | Dev-only fake library generator (5,000 games) for performance checks. |

## Data model

SQLite tables (migrations 1 and 2):

```sql
games(
  id TEXT PRIMARY KEY,            -- UUID v4
  name TEXT NOT NULL,
  sorting_name TEXT NOT NULL,     -- derived (articles stripped) unless user overrides
  search_key TEXT NOT NULL,       -- Unicode-lowercased name for case-insensitive search
  source TEXT NOT NULL,           -- provider id: 'manual', 'steam', ...
  source_game_id TEXT,            -- store id; NULL for manual games
  installed INTEGER NOT NULL,     -- 0 when a re-scan no longer finds the game
  install_dir TEXT,
  play_action TEXT,               -- JSON PlayAction
  playtime_seconds INTEGER NOT NULL DEFAULT 0,
  play_count INTEGER NOT NULL DEFAULT 0,
  last_played TEXT,               -- RFC 3339 UTC
  date_added TEXT NOT NULL,
  favorite INTEGER NOT NULL DEFAULT 0,
  hidden INTEGER NOT NULL DEFAULT 0,
  cover_path TEXT, icon_path TEXT, background_path TEXT, -- relative to the data dir
  description TEXT, release_date TEXT,            -- store metadata (migration 2)
  developers TEXT, publishers TEXT, genres TEXT,  -- JSON string arrays, default '[]'
  metadata_updated_at TEXT,       -- NULL = never fetched
  extra TEXT NOT NULL DEFAULT '{}', -- free-form JSON
  UNIQUE(source, source_game_id)
)
play_sessions(id, game_id, started_at, last_seen_at, ended_at, duration_seconds)
schema_version(version INTEGER)
```

`PlayAction` (JSON): `{ kind: "file" | "uri", target, args, working_dir, tracking: "default" |
"process" | "directory" | "name", tracking_path }`. `default` means: process tree for `file`
actions, install directory for `uri` actions.

Re-scan semantics: `upsert_imported(source, games)` matches on `(source, source_game_id)`,
updates store-owned fields (install dir, action, installed flag; name only if the user has not
renamed it, tracked via `extra.user_renamed`), inserts new games, and marks games of that source
that were not found as `installed = 0` (playtime and favorites are kept). Store playtime merges as
the maximum of the stored and the store value, last played as the later one.

A scan combines `scan_installed()` with `scan_owned(config)` when the provider supports it and is
configured: owned games missing from the installed list are imported with `installed = false` and
get the provider's `install_action()` instead of Play. A missing configuration (`NotConfigured`)
silently falls back to installed games only.

## Event flow

1. Core operations publish `CoreEvent`s on the `EventBus`: `GamesChanged { ids }`,
   `GameRemoved { id }`, `LibraryScanned { provider, added, updated, uninstalled }`,
   `GameStarted { id }`, `GameStopped { id, session_seconds }`, `CoverUpdated { id }`.
2. `src-tauri` subscribes once at startup and emits each event to the webview as `core-event`.
3. The UI store listens and refreshes the affected games.

## Launch and playtime

- `Launcher::launch(game)` resolves the action, starts it, inserts a `play_sessions` row and
  spawns one tracker thread. A game can have at most one active session.
- The tracker polls every 2 s via `ProcessProbe` (process tree of the spawned PID, or any process
  whose executable is under the tracking directory). URI launches wait up to 90 s for a matching
  process to appear before giving up.
- Every poll updates `last_seen_at`. On stop: `ended_at` is set, duration is added to
  `playtime_seconds`, `last_played` and `play_count` update, `GameStopped` fires.
- App exit: `Launcher::shutdown()` signals all trackers and closes their sessions with elapsed time.
- Crash recovery: on startup, open sessions are closed using `last_seen_at`.

## Commands (`src-tauri/src/commands.rs`)

| Command | Purpose |
|---|---|
| `list_games(query)` | Filtered, sorted list (`GameQuery`: search, sort, direction, sources, favorites_only, show_hidden, installed_only). |
| `get_game(id)` | One game. |
| `add_manual_game(input)` | Add a game from an executable path. |
| `update_game(id, update)` | Rename, favorite, hidden, play action, cover. |
| `remove_game(id)` | Delete game (and its cached images). |
| `list_providers()` | Registered providers. |
| `scan_library(provider_id)` | Scan installed (and owned) games, upsert, queue covers and metadata. |
| `launch_game(id)` / `running_games()` | Launch and active sessions. |
| `install_game(id)` | Opens the provider's install action (e.g. `steam://install/440`). |
| `refresh_metadata(id)` | Queues a metadata (and missing background) download for one game. |
| `refetch_artwork(id)` | Clears the SteamGridDB "checked" markers and re-fetches cover and background from SteamGridDB, replacing the current ones. Errors without an API key. |
| `app_info()` | Version, data dir, log path. |
| `seed_fake_library(count)` | Debug builds only. |

Payload types are generated into `ui/src/bindings/` by `ts-rs` during `cargo test`. The check
script fails if the generated files differ from what's committed.

## Performance

The grid renders only the rows in view (plus 3 rows of overscan), so about 30 to 45 tiles exist in
the DOM regardless of library size. Rows have a fixed height derived from the container width,
which makes the visible window plain arithmetic (`ui/src/lib/virtualRows.ts`).

### Measurements (2026-10-18)

Machine: Windows 11 desktop, 240 Hz display, release build (`tauri build --no-bundle`), portable
data dir with **5,000 games** (4,948 seeded fake games + 52 real Steam games), measured via the
WebView2 DevTools protocol (scripted scrolling and typing in the real UI).

| Metric | Result |
|---|---|
| Cold start: process start until the first cover tiles are in the DOM | **480 to 600 ms** (3 runs) |
| Idle memory, app process (Rust) | **36 MB** working set |
| Idle memory, app + 6 WebView2 processes | **200 MB** private, 374 MB working set (shared pages counted per process) |
| JS heap | 11 to 32 MB |
| Scrolling 3 s at ~3,600 px/s | 721 frames, avg **4.2 ms**, p95 4.3 ms, max 4.4 ms, 0 frames over 33 ms |
| `list_games` for all 5,000 games (query + IPC + JSON) | 66 ms (160 ms in a debug build) |
| Search as you type, end to end (includes the 80 ms debounce) | 87 to 91 ms for selective terms, 145 to 150 ms for terms matching ~4,000+ games |

Honest notes:

- Broad queries are dominated by serializing every matching game over IPC. If that ever matters,
  return a slim list payload (id, name, cover, flags) and fetch full games on demand, or filter
  client-side.
- WebView2 dominates memory, as with any Tauri app on Windows; the Rust side stays small.
- Steam scan of 52 installed games plus cover lookup runs on a background thread at startup and
  does not delay the first paint.
