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
| `providers/` | `LibraryProvider` trait, `ProviderRegistry`, `steam` (VDF/ACF parser, library discovery, tool filter), `manual` helpers, `epic`/`gog` (Windows launchers; Heroic/Legendary on Linux through the shared `heroic` module), `xbox` (installed games from AppX manifests; Store catalog lookups in `catalog.rs` give portrait covers, backgrounds and metadata; optional account in `account.rs`), `lutris` (Linux, reads `pga.db`). |
| `auth/` | Store account sign-in. `TokenStore` (`KeyringTokenStore` = system keyring, `MemoryTokenStore` for tests), `AccountSession` (lazy token load, refresh within 60 s of expiry or after a 401 with one retry, sign-out when the store rejects the refresh token), the injectable `Http` trait (`UreqHttp` redacts request secrets from errors), `AccountAuth` (what a provider exposes: status, login URL, code sign-in, sign-out) and `AccountStatus` (UI contract). GOG (`providers/gog/account.rs`) Epic (`providers/epic/account.rs` + `web.rs`) and Xbox (`providers/xbox/account.rs`: Microsoft account, then Xbox user token and XSTS per scan, title history and playtime) implement it. |
| `launcher/` | Starts games (`file` spawn or `uri` open), tracks sessions on a background thread per game, writes playtime. `ProcessProbe` trait keeps tracking testable. |
| `images/` | Image cache under `images/covers`, `images/backgrounds` and `images/icons`. `CoverFetcher` (4 workers) tries each provider's cover candidates, local files before CDN URLs. `MetadataFetcher` (1 worker, at least 1.5 s between store requests) stores store metadata, a background and the tray icon per game (see "Game icons" below). `SteamGridDb` (`images/steamgriddb.rs`) is a fallback source for covers and backgrounds when a user API key is set: used when the provider has no image (or only low-quality ones, see `covers_are_low_quality`), and for manual games. Global limiter of 2 requests/s, "nothing found" remembered in `games.extra`, key shared via `Library::artwork()` and updated by `save_settings`. |
| `settings` | Small `settings.json` (serde, defaults on missing/corrupt file), including a per-provider config map (`providers.steam = { apiKey, steamId }`). |
| `events` | `EventBus` with serializable `CoreEvent` enum; subscribers are callbacks. Plugin hooks attach here later. |
| `platform/` | OS-specific bits: Steam install dirs (registry on Windows, XDG paths on Linux), opening URIs, process snapshots, installed store clients (`installed_store_clients`: Steam, Epic, GOG Galaxy, Xbox). Stubs on other targets. |
| `tray` | Pure logic for the tray and window lifecycle: which games the tray menu lists (`tray_games`), close/minimize decisions, when an exit request is ignored, `--minimized` argument parsing. No Tauri dependency, fully unit-tested. |
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

Installed games can be uninstalled through `Library::uninstall_game`, which runs the provider's
`uninstall_action()` (Steam `steam://uninstall/<id>`, GOG the game's `unins000.exe`, the Epic
launcher's library page, Xbox the Windows "Installed apps" settings). `ProviderInfo.can_uninstall`
tells the UI whether to offer the button. Questkeep never deletes files or writes to the database
for this; the store asks for confirmation and the next scan marks the game as not installed.

## Event flow

1. Core operations publish `CoreEvent`s on the `EventBus`: `GamesChanged { ids }`,
   `GameRemoved { id }`, `LibraryScanned { provider, added, updated, uninstalled }`,
   `GameStarted { id }`, `GameStopped { id, session_seconds }`, `CoverUpdated { id }`.
2. `src-tauri` subscribes once at startup and emits each event to the webview as `core-event`.
3. The UI store listens and refreshes the affected games.

## Tray and window lifecycle (`src-tauri/src/{tray,window,autostart}.rs`)

```
 close button ── closeToTray? ──yes──▶ destroy main webview ──▶ app waits in the tray
 minimize     ── minimizeToTray? ─yes─▶ (WebView2 renderer freed)   core keeps running
                                              ▲                      (launcher, fetchers)
 tray click / "Open Questkeep" / 2nd start ───┘ create_main_window
 tray "Exit" ─▶ quit flag + app.exit(0) ─▶ RunEvent::Exit ─▶ close fetchers, save sessions
```

- The tray icon is created in `setup`. Left click opens the window. Right click opens, on
  Windows, the **tray popup** (below); on Linux the native menu (recent games, Favorites submenu,
  Open Questkeep, Store clients submenu, Exit). The model behind both is kept by one background
  thread that blocks on a channel fed by
  an `EventBus` subscriber (`GamesChanged`, `GameRemoved`, `GameStarted`, `GameStopped`,
  `LibraryScanned`). After a trigger it waits 2 s, coalesces further triggers, recomputes the menu
  model and skips the rebuild when it is unchanged; menus are applied on the main thread (Linux) or
  the model is stored for the next popup (Windows). Idle cost is zero: no timers, no polling (the
  launcher only polls while a game runs).
- **Tray popup** (`src-tauri/src/tray_popup/`, Windows only). A borderless Win32 window drawn with
  Direct2D/DirectWrite, created on its own thread (own message loop) when opened and destroyed when
  it closes (Escape, losing activation, or after a choice; the action runs afterwards). Recent
  games are cover tiles (up to 8), favorites are rows with icons (scrolling past 6), then store
  pills and Open/Exit. `layout.rs` is pure and tested (layout, hit testing, keyboard navigation,
  placement next to the taskbar); `render.rs` draws onto any Direct2D target, which the
  `renders_offscreen` test uses to render into a WIC bitmap (set `QUESTKEEP_POPUP_PREVIEW=<png>`
  to save it, `QUESTKEEP_POPUP_SCALE` for the DPI scale, `QUESTKEEP_POPUP_IMAGES=<data dir>/images`
  for real covers); `window.rs` owns the window and input. `a11y.rs` builds an AccessKit tree from
  the layout (groups per section, a named button per item, keyboard focus); `window.rs` serves it
  on `WM_GETOBJECT` through `accesskit_windows` and forwards screen-reader actions (invoke,
  focus, scroll into view) to the window thread. A test reads it back through the UI Automation
  client API on a window that is never shown.
  Images are scaled once to their exact pixel size (`images::load_image_filled`, `scale_icon`) and
  drawn 1:1 at pixel-snapped positions, so nothing is resampled while drawing.
- **Game icons.** Linux menu entries are `IconMenuItem`s with a 32x32 RGBA icon when the game has a cached
  one, plain text items otherwise. The menu build only decodes those small PNGs
  (`images::load_icon_rgba`, at most 10 recent + 30 favorites per rebuild); it never produces an
  icon. When the model lists a game with no icon, the tray thread queues an icon job
  (`MetadataFetcher::enqueue_icons`, deduplicated per run); the worker stores the icon, sets
  `games.icon_path` and emits `GamesChanged`, so the next debounced rebuild shows it. The icon
  path is part of `TrayGame`, so a new icon counts as a changed model.
- Hiding **destroys** the main window instead of hiding it. `RunEvent::ExitRequested` with no
  exit code (last window gone) is ignored while the `hidden_to_tray` flag is set and no real exit
  was requested; `app.exit(n)` always goes through. Store login windows (`signin-*`) are neither
  closed nor handled by this logic. If the tray icon cannot be created, the close button quits.
- Core events are emitted to the `main` window only; with no window that is a no-op. The UI
  reloads games and running sessions when a new window starts, so nothing is lost while away.
- Start with system: `tauri-plugin-autostart` registers `questkeep --minimized`, driven from
  `save_settings` (the UI only toggles `startWithSystem`). With `--minimized` and
  `startMinimized` on, startup scans run but no window is created.
  `tauri-plugin-single-instance` (release builds) shows the running instance on a second start.
- Settings: `closeToTray` (default on), `minimizeToTray`, `startWithSystem`, `startMinimized`.

## Game icons (`images/icons.rs`, `MetadataFetcher`)

`images/icons/<game id>.png` is always a 32x32 RGBA PNG. Every source is decoded with the `image`
crate (PNG, JPEG, ICO only; size and allocation limits), made square, scaled with Lanczos3 on
premultiplied alpha and re-encoded. Sources, best first (the first valid one wins):

1. The provider: `LibraryProvider::icon_candidates`. Steam: the square `<sha1>.jpg` in
   `appcache/librarycache/<appid>/` (also the old flat `<appid>_icon.jpg` and hash sub-folders; by
   name pattern first, else the smallest square image of 16-256 px). GOG: `_links.iconSquare` /
   `_links.icon` of the cached store entry. Epic: the catalog `Thumbnail` key image. Provider
   images must be nearly square (ratio <= 1.15), otherwise they are skipped.
2. The executable's icon for games with a `file` action (Windows: `platform::executable_icon`,
   `PrivateExtractIconsW` at 48 px, then `GetIconInfo` + `GetDIBits`; the AND mask supplies alpha for
   icons without an alpha channel). It lives in `platform/exe_icon.rs`, one of the few `unsafe` modules (see DECISIONS.md).
3. A top-aligned square crop of the cached cover.

Jobs run on the metadata worker, as the last step of a metadata job, and on demand for games the
tray lists (`enqueue_icons`). A game that failed to get an icon and has no cover yet is retried
when it is queued again (the cover arriving emits `GamesChanged`, the tray requeues it).

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
| `account_status()` | `AccountStatus[]` for the stores with an account (GOG, Epic): signed in, display name, whether pasting a code is supported and where to get it. |
| `sign_in(provider)` | Opens the store login window (no IPC access) and resolves `true` once the captured code was exchanged, the store scanned and downloads queued; `false` if the window was closed. Errors if a window for that store is already open. |
| `sign_in_with_code(provider, code)` | Same without the window: a pasted code (GOG: also the whole `on_login_success` address; Epic: the `authorizationCode` value or the page JSON). Returns the new `AccountStatus`. |
| `sign_out(provider)` | Deletes the stored tokens; returns the new `AccountStatus`. Games stay in the library. |
| `app_info()` | Version, data dir, log path. |
| `open_app_folder(kind)` | Opens the `"data"` or `"logs"` folder in the file manager from Rust (the opener plugin's JS API is not given path access). Other kinds are rejected. |
| `seed_fake_library(count)` | Debug builds only. |

### Store accounts (GOG, Epic)

Tokens live only in the system keyring (service `dev.masch0212.questkeep`, user = provider id); the
stored blob holds the refresh token, account id and display name, never the access token (kept in
memory; Epic's is longer than the Windows credential limit). The login window is a private-profile
webview on the store's own login page; the shell cancels the redirect that carries the one-time
code and passes the code to `Library::sign_in_with_code`, which exchanges it, saves the tokens and
scans the provider. Providers get their `AccountSession` by constructor injection
(`ProviderRegistry::with_builtin(Arc<dyn TokenStore>)`); signed out, `scan_owned` returns
`NotConfigured` so `Library::scan` silently stays installed-only. Epic metadata then returns an
error so it is retried after sign-in, but Epic covers, backgrounds and icons of installed games
still work: `providers/epic/public.rs` looks them up by the manifest's namespace and catalog item
id with an in-memory app-only token (`client_credentials`); a signed-in account is tried first.

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
